use std::{
    collections::VecDeque,
    process::Command,
    time::{Duration, Instant},
};

use eframe::egui::{
    self, Color32, CornerRadius, ProgressBar, RichText, Sense, Stroke, StrokeKind, Vec2,
};

const ACCENT: Color32 = Color32::from_rgb(118, 185, 0);
const CYAN: Color32 = Color32::from_rgb(0, 180, 216);
const ORANGE: Color32 = Color32::from_rgb(255, 159, 28);
const RED: Color32 = Color32::from_rgb(230, 57, 70);
const MUTED: Color32 = Color32::from_rgb(140, 148, 164);
const CARD: Color32 = Color32::from_rgb(26, 30, 42);
const BORDER: Color32 = Color32::from_rgb(48, 54, 72);
const QUERY: &str = "name,temperature.gpu,utilization.gpu,utilization.memory,memory.used,memory.total,fan.speed,power.draw,power.limit,power.min_limit,power.max_limit,power.default_limit,clocks.gr,clocks.mem,clocks.max.gr,clocks.max.mem,pcie.link.gen.current,pcie.link.width.current,driver_version,vbios_version";
const FAN_CONTROL_TARGET: &str = "[gpu:0]/GPUFanControlState";
const FAN_SPEED_TARGET: &str = "[fan:0]/GPUTargetFanSpeed";
const CORE_OFFSET_TARGET: &str = "[gpu:0]/GPUGraphicsClockOffsetAllPerformanceLevels";
const MEMORY_OFFSET_TARGET: &str = "[gpu:0]/GPUMemoryTransferRateOffsetAllPerformanceLevels";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NVIDIA GPU Manager")
            .with_inner_size([920.0, 680.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "NVIDIA GPU Manager",
        options,
        Box::new(|cc| Ok(Box::new(ManagerApp::new(cc)))),
    )
}

#[derive(Clone, Debug, Default)]
struct GpuStats {
    name: String,
    temp: f32,
    gpu_util: f32,
    mem_util: f32,
    mem_used: f32,
    mem_total: f32,
    fan: f32,
    power_draw: f32,
    power_limit: f32,
    power_min: f32,
    power_max: f32,
    power_default: f32,
    clk_gpu: f32,
    clk_mem: f32,
    clk_gpu_max: f32,
    clk_mem_max: f32,
    pcie_gen: String,
    pcie_width: String,
    driver: String,
    vbios: String,
}

impl GpuStats {
    fn parse_csv(raw: &str) -> Result<Self, String> {
        let line = raw.lines().next().ok_or("nvidia-smi returned no GPU")?;
        let values: Vec<_> = line.split(',').map(str::trim).collect();
        if values.len() != 20 {
            return Err(format!(
                "expected 20 values from nvidia-smi, received {}",
                values.len()
            ));
        }
        let number = |index: usize| parse_number(values[index]);
        Ok(Self {
            name: values[0].to_owned(),
            temp: number(1),
            gpu_util: number(2),
            mem_util: number(3),
            mem_used: number(4),
            mem_total: number(5),
            fan: number(6),
            power_draw: number(7),
            power_limit: number(8),
            power_min: number(9),
            power_max: number(10),
            power_default: number(11),
            clk_gpu: number(12),
            clk_mem: number(13),
            clk_gpu_max: number(14),
            clk_mem_max: number(15),
            pcie_gen: values[16].to_owned(),
            pcie_width: values[17].to_owned(),
            driver: values[18].to_owned(),
            vbios: values[19].to_owned(),
        })
    }
}

fn parse_number(value: &str) -> f32 {
    value.parse().unwrap_or(0.0)
}

fn read_stats() -> Result<GpuStats, String> {
    let output = Command::new(nvidia_smi_path())
        .args([
            format!("--query-gpu={QUERY}"),
            "--format=csv,noheader,nounits".to_owned(),
        ])
        .output()
        .map_err(|error| format!("could not start nvidia-smi: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    GpuStats::parse_csv(&String::from_utf8_lossy(&output.stdout))
}

fn nvidia_smi_path() -> &'static str {
    for path in [
        "/run/current-system/sw/bin/nvidia-smi",
        "/run/opengl-driver/bin/nvidia-smi",
    ] {
        if std::path::Path::new(path).exists() {
            return path;
        }
    }
    "nvidia-smi"
}

fn privileged_smi(args: &[String]) -> Result<(), String> {
    let output = Command::new("pkexec")
        .arg(nvidia_smi_path())
        .args(args)
        .output()
        .map_err(|error| format!("could not start pkexec: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    Err(format!(
        "nvidia-smi rejected the requested change: {detail}"
    ))
}

#[derive(Clone, Debug)]
struct SettingRange {
    current: i32,
    min: i32,
    max: i32,
}

#[derive(Clone, Debug, Default)]
struct GpuControls {
    fan_speed: Option<SettingRange>,
    fan_manual: bool,
    core_offset: Option<SettingRange>,
    memory_offset: Option<SettingRange>,
}

impl GpuControls {
    fn read() -> Self {
        Self {
            fan_speed: query_setting_range(FAN_SPEED_TARGET),
            fan_manual: query_setting_value(FAN_CONTROL_TARGET).unwrap_or(0) != 0,
            core_offset: query_setting_range(CORE_OFFSET_TARGET),
            memory_offset: query_setting_range(MEMORY_OFFSET_TARGET),
        }
    }
}

fn run_nvidia_settings(args: &[String]) -> Result<String, String> {
    let output = Command::new("nvidia-settings")
        .args(args)
        .output()
        .map_err(|error| format!("could not start nvidia-settings: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "nvidia-settings rejected the change: {}",
            error.trim()
        ))
    }
}

fn query_setting_value(target: &str) -> Option<i32> {
    let output = run_nvidia_settings(&["-q".to_owned(), target.to_owned()]).ok()?;
    parse_setting_value(&output)
}

fn query_setting_range(target: &str) -> Option<SettingRange> {
    let output = run_nvidia_settings(&["-q".to_owned(), target.to_owned()]).ok()?;
    let current = parse_setting_value(&output)?;
    let (min, max) = parse_setting_range(&output)?;
    Some(SettingRange { current, min, max })
}

fn parse_setting_value(output: &str) -> Option<i32> {
    output
        .lines()
        .find(|line| line.trim_start().starts_with("Attribute '"))
        .and_then(|line| line.rsplit_once(": "))
        .and_then(|(_, value)| value.trim().trim_end_matches('.').parse().ok())
}

fn parse_setting_range(output: &str) -> Option<(i32, i32)> {
    let range = output
        .lines()
        .find_map(|line| line.split_once("range ").map(|(_, range)| range))?
        .split_once(" (inclusive)")?
        .0;
    let (min, max) = range.split_once(" - ")?;
    Some((min.parse().ok()?, max.parse().ok()?))
}

fn assign_nvidia_setting(target: &str, value: i32) -> Result<(), String> {
    run_nvidia_settings(&["-a".to_owned(), format!("{target}={value}")]).map(|_| ())
}

fn set_fan_speed(speed: u32) -> Result<(), String> {
    if speed == 0 {
        assign_nvidia_setting(FAN_CONTROL_TARGET, 0)
    } else {
        assign_nvidia_setting(FAN_CONTROL_TARGET, 1)?;
        assign_nvidia_setting(FAN_SPEED_TARGET, speed as i32)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Monitoring,
    Power,
    Fan,
    Tuning,
    Info,
}

struct ManagerApp {
    stats: Option<GpuStats>,
    history_temp: VecDeque<f32>,
    history_gpu: VecDeque<f32>,
    history_power: VecDeque<f32>,
    history_fan: VecDeque<f32>,
    last_refresh: Instant,
    status: String,
    tab: Tab,
    power_limit: u32,
    fan_speed: u32,
    fan_manual: bool,
    controls: GpuControls,
    core_offset: i32,
    memory_offset: i32,
}

impl ManagerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        let controls = GpuControls::read();
        let mut app = Self {
            stats: None,
            history_temp: empty_history(),
            history_gpu: empty_history(),
            history_power: empty_history(),
            history_fan: empty_history(),
            last_refresh: Instant::now() - Duration::from_secs(2),
            status: "Starting...".to_owned(),
            tab: Tab::Monitoring,
            power_limit: 250,
            fan_speed: controls
                .fan_speed
                .as_ref()
                .map_or(30, |setting| setting.current.max(setting.min) as u32),
            fan_manual: controls.fan_manual,
            core_offset: controls
                .core_offset
                .as_ref()
                .map_or(0, |setting| setting.current),
            memory_offset: controls
                .memory_offset
                .as_ref()
                .map_or(0, |setting| setting.current),
            controls,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.last_refresh = Instant::now();
        match read_stats() {
            Ok(stats) => {
                if self.stats.is_none() && stats.power_limit > 0.0 {
                    self.power_limit = stats.power_limit as u32;
                }
                push(&mut self.history_temp, stats.temp);
                push(&mut self.history_gpu, stats.gpu_util);
                push(&mut self.history_power, stats.power_draw);
                push(&mut self.history_fan, stats.fan);
                self.status = format!(
                    "Live  |  Temp: {:.0} C  |  GPU: {:.0}%  |  VRAM: {:.0}%  |  Power: {:.0}/{:.0} W  |  Fan: {:.0}%",
                    stats.temp, stats.gpu_util, stats.mem_util, stats.power_draw, stats.power_limit, stats.fan
                );
                self.stats = Some(stats);
            }
            Err(error) => self.status = format!("nvidia-smi unavailable: {error}"),
        }
    }

    fn apply_power_limit(&mut self) {
        self.status = format!("Setting power limit to {} W...", self.power_limit);
        match privileged_smi(&[
            "-i".to_owned(),
            "0".to_owned(),
            "-pl".to_owned(),
            self.power_limit.to_string(),
        ]) {
            Ok(()) => {
                self.status = format!("Power limit set to {} W", self.power_limit);
                self.refresh();
            }
            Err(error) => self.status = format!("Power limit failed: {error}"),
        }
    }

    fn reset_power_limit(&mut self) {
        let Some(stats) = &self.stats else {
            return;
        };
        if stats.power_default <= 0.0 {
            self.status = "The driver did not report a default power limit".to_owned();
            return;
        }
        self.power_limit = stats.power_default as u32;
        self.apply_power_limit();
    }

    fn apply_fan_speed(&mut self, speed: u32) {
        self.status = format!("Setting fan speed to {speed}%...");
        match set_fan_speed(speed) {
            Ok(()) => {
                self.fan_manual = speed > 0;
                self.controls = GpuControls::read();
                self.status = if speed == 0 {
                    "Automatic fan control enabled".to_owned()
                } else {
                    format!("Fan speed set to {speed}%")
                };
                self.refresh();
            }
            Err(error) => {
                self.status = format!(
                "Fan control failed: {error}. This feature is not supported by every driver/GPU."
            )
            }
        }
    }

    fn apply_tuning(&mut self) {
        let result: Result<(), String> = (|| {
            if self.controls.core_offset.is_some() {
                assign_nvidia_setting(CORE_OFFSET_TARGET, self.core_offset)?;
            }
            if self.controls.memory_offset.is_some() {
                assign_nvidia_setting(MEMORY_OFFSET_TARGET, self.memory_offset)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.controls = GpuControls::read();
                self.status = format!(
                    "Tuning applied  |  Core offset: {:+} MHz  |  Memory transfer offset: {:+} MHz",
                    self.core_offset, self.memory_offset
                );
            }
            Err(error) => self.status = format!("Tuning failed: {error}"),
        }
    }

    fn reset_tuning(&mut self) {
        self.core_offset = 0;
        self.memory_offset = 0;
        self.apply_tuning();
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new(
                    self.stats
                        .as_ref()
                        .map_or("NVIDIA GPU Manager", |stats| stats.name.as_str()),
                )
                .color(ACCENT),
            );
            ui.add_space(16.0);
            for (tab, label) in [
                (Tab::Monitoring, "MONITORING"),
                (Tab::Power, "POWER"),
                (Tab::Fan, "FAN"),
                (Tab::Tuning, "TUNING"),
                (Tab::Info, "INFO"),
            ] {
                if ui.selectable_label(self.tab == tab, label).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.separator();
    }

    fn monitoring(&self, ui: &mut egui::Ui, stats: &GpuStats) {
        ui.columns(4, |columns| {
            stat_card(
                &mut columns[0],
                "TEMPERATURE",
                stats.temp,
                "C",
                RED,
                100.0,
                &self.history_temp,
            );
            stat_card(
                &mut columns[1],
                "GPU LOAD",
                stats.gpu_util,
                "%",
                ACCENT,
                100.0,
                &self.history_gpu,
            );
            stat_card(
                &mut columns[2],
                "POWER",
                stats.power_draw,
                "W",
                ORANGE,
                500.0,
                &self.history_power,
            );
            stat_card(
                &mut columns[3],
                "FAN",
                stats.fan,
                "%",
                CYAN,
                100.0,
                &self.history_fan,
            );
        });
        ui.add_space(12.0);
        card(ui, "MEMORY & LOAD", |ui| {
            metric_bar(
                ui,
                "GPU",
                stats.gpu_util / 100.0,
                format!("{:.0}%", stats.gpu_util),
            );
            let mem_ratio = if stats.mem_total > 0.0 {
                stats.mem_used / stats.mem_total
            } else {
                0.0
            };
            metric_bar(
                ui,
                "VRAM",
                mem_ratio,
                format!("{:.0} / {:.0} MiB", stats.mem_used, stats.mem_total),
            );
            metric_bar(
                ui,
                "Temp",
                stats.temp / 100.0,
                format!("{:.0} C", stats.temp),
            );
        });
        ui.add_space(12.0);
        card(ui, "CLOCK FREQUENCIES", |ui| {
            egui::Grid::new("clocks")
                .spacing([28.0, 8.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.colored_label(MUTED, "Current");
                    ui.colored_label(MUTED, "Maximum");
                    ui.end_row();
                    ui.label("GPU core");
                    ui.colored_label(ACCENT, format!("{:.0} MHz", stats.clk_gpu));
                    ui.label(format!("{:.0} MHz", stats.clk_gpu_max));
                    ui.end_row();
                    ui.label("Memory");
                    ui.colored_label(CYAN, format!("{:.0} MHz", stats.clk_mem));
                    ui.label(format!("{:.0} MHz", stats.clk_mem_max));
                    ui.end_row();
                });
        });
    }

    fn power(&mut self, ui: &mut egui::Ui, stats: &GpuStats) {
        card(ui, "CURRENT POWER USAGE", |ui| {
            ui.colored_label(
                ORANGE,
                RichText::new(format!("{:.1} W", stats.power_draw)).size(25.0),
            );
            ui.label(format!("Current limit: {:.0} W", stats.power_limit));
            ui.colored_label(
                MUTED,
                format!("Range: {:.0} W / {:.0} W", stats.power_min, stats.power_max),
            );
        });
        ui.add_space(12.0);
        card(ui, "SET POWER LIMIT", |ui| {
            let min = stats.power_min.max(1.0) as u32;
            let max = stats.power_max.max(min as f32) as u32;
            ui.colored_label(
                ORANGE,
                RichText::new(format!("{} W", self.power_limit)).size(23.0),
            );
            ui.add(egui::Slider::new(&mut self.power_limit, min..=max));
            ui.horizontal(|ui| {
                if ui.button("Apply limit").clicked() {
                    self.apply_power_limit();
                }
                if ui.button("Reset to default").clicked() {
                    self.reset_power_limit();
                }
            });
            ui.colored_label(MUTED, "Changes require Polkit authentication.");
        });
    }

    fn fan(&mut self, ui: &mut egui::Ui, stats: &GpuStats) {
        card(ui, "FAN STATUS", |ui| {
            ui.colored_label(CYAN, RichText::new(format!("{:.0}%", stats.fan)).size(25.0));
            ui.label(if self.fan_manual {
                "Mode: manual"
            } else {
                "Mode: automatic"
            });
        });
        ui.add_space(12.0);
        card(ui, "MANUAL FAN CONTROL", |ui| {
            ui.colored_label(
                CYAN,
                RichText::new(format!("{}%", self.fan_speed)).size(23.0),
            );
            let range = self
                .controls
                .fan_speed
                .as_ref()
                .map(|setting| (setting.min, setting.max));
            let min = range.map_or(30, |(min, _)| min.max(0) as u32);
            let max = range.map_or(100, |(_, max)| max.max(0) as u32);
            ui.add(egui::Slider::new(&mut self.fan_speed, min..=max));
            ui.horizontal(|ui| {
                if ui.button("Apply fan speed").clicked() {
                    self.apply_fan_speed(self.fan_speed);
                }
                if ui.button("Automatic").clicked() {
                    self.apply_fan_speed(0);
                }
            });
            ui.colored_label(
                MUTED,
                "Fan control uses nvidia-settings and requires X11 plus Coolbits=4.",
            );
            if let Some((min, max)) = range {
                ui.colored_label(MUTED, format!("Driver range for this GPU: {min}-{max}%."));
            }
        });
    }

    fn tuning(&mut self, ui: &mut egui::Ui) {
        card(ui, "GPU-SPECIFIC TUNING", |ui| {
            ui.colored_label(
                ORANGE,
                "Clock offsets can make the system unstable. Increase values gradually.",
            );
            ui.add_space(8.0);
            if let Some(range) = &self.controls.core_offset {
                ui.label(format!("GPU core offset: {:+} MHz", self.core_offset));
                ui.add(egui::Slider::new(
                    &mut self.core_offset,
                    range.min..=range.max,
                ));
                ui.colored_label(
                    MUTED,
                    format!("Driver range: {:+} to {:+} MHz", range.min, range.max),
                );
            } else {
                ui.colored_label(MUTED, "GPU core offset is not exposed by the driver.");
            }
            ui.add_space(8.0);
            if let Some(range) = &self.controls.memory_offset {
                ui.label(format!(
                    "Memory transfer offset: {:+} MHz",
                    self.memory_offset
                ));
                ui.add(egui::Slider::new(
                    &mut self.memory_offset,
                    range.min..=range.max,
                ));
                ui.colored_label(
                    MUTED,
                    format!("Driver range: {:+} to {:+} MHz", range.min, range.max),
                );
            } else {
                ui.colored_label(MUTED, "Memory offset is not exposed by the driver.");
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Apply offsets").clicked() {
                    self.apply_tuning();
                }
                if ui.button("Reset offsets").clicked() {
                    self.reset_tuning();
                }
            });
            ui.colored_label(
                MUTED,
                "Offset changes use nvidia-settings and normally require Coolbits=8.",
            );
        });
    }

    fn info(&self, ui: &mut egui::Ui, stats: &GpuStats) {
        card(ui, "GPU INFORMATION", |ui| {
            egui::Grid::new("info").spacing([28.0, 8.0]).show(ui, |ui| {
                for (label, value) in [
                    ("GPU model", stats.name.clone()),
                    ("Driver", stats.driver.clone()),
                    ("VRAM total", format!("{:.0} MiB", stats.mem_total)),
                    ("PCIe generation", format!("Gen {}", stats.pcie_gen)),
                    ("PCIe width", format!("x{}", stats.pcie_width)),
                    ("VBIOS version", stats.vbios.clone()),
                ] {
                    ui.colored_label(MUTED, label);
                    ui.label(value);
                    ui.end_row();
                }
            });
        });
        ui.add_space(12.0);
        card(ui, "NIXOS INTEGRATION", |ui| {
            ui.label("Use the included NixOS module. It installs the app and enables Polkit.");
            ui.colored_label(MUTED, "No passwordless sudo rule is required.");
        });
    }
}

impl eframe::App for ManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_refresh.elapsed() >= Duration::from_millis(1500) {
            self.refresh();
        }
        ctx.request_repaint_after(Duration::from_millis(250));

        egui::CentralPanel::default().show(ctx, |ui| {
            self.top_bar(ui);
            ui.add_space(8.0);
            if let Some(stats) = self.stats.clone() {
                match self.tab {
                    Tab::Monitoring => self.monitoring(ui, &stats),
                    Tab::Power => self.power(ui, &stats),
                    Tab::Fan => self.fan(ui, &stats),
                    Tab::Tuning => self.tuning(ui),
                    Tab::Info => self.info(ui, &stats),
                }
            } else {
                ui.colored_label(RED, "No NVIDIA GPU data available.");
                ui.label(
                    "Check that the proprietary NVIDIA driver is active and nvidia-smi works.",
                );
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.separator();
                ui.colored_label(MUTED, &self.status);
            });
        });
    }
}

fn empty_history() -> VecDeque<f32> {
    VecDeque::from(vec![0.0; 60])
}

fn push(history: &mut VecDeque<f32>, value: f32) {
    if history.len() == 60 {
        history.pop_front();
    }
    history.push_back(value);
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(13, 15, 20);
    visuals.window_fill = CARD;
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.selection.bg_fill = ACCENT;
    ctx.set_visuals(visuals);
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.colored_label(MUTED, RichText::new(title).strong());
            ui.add_space(7.0);
            body(ui);
        });
}

fn stat_card(
    ui: &mut egui::Ui,
    title: &str,
    value: f32,
    unit: &str,
    color: Color32,
    max: f32,
    history: &VecDeque<f32>,
) {
    card(ui, title, |ui| {
        ui.colored_label(
            color,
            RichText::new(format!("{value:.0} {unit}"))
                .size(24.0)
                .strong(),
        );
        sparkline(ui, history, max, color);
    });
}

fn sparkline(ui: &mut egui::Ui, values: &VecDeque<f32>, max: f32, color: Color32) {
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 62.0), Sense::hover());
    let rect = response.rect;
    painter.rect(
        rect,
        CornerRadius::same(4),
        CARD,
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    if values.len() < 2 {
        return;
    }
    let points: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = rect.left() + rect.width() * index as f32 / (values.len() - 1) as f32;
            let y = rect.bottom() - rect.height() * (value / max).clamp(0.0, 1.0);
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(2.0, color)));
}

fn metric_bar(ui: &mut egui::Ui, label: &str, ratio: f32, text: String) {
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.label(format!("{label}:"));
        ui.add(ProgressBar::new(ratio.clamp(0.0, 1.0)).text(text));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gpu_stats() {
        let stats = GpuStats::parse_csv(
            "NVIDIA RTX Test, 55, 71, 12, 1024, 8192, 44, 120.5, 250, 100, 300, 250, 1800, 7000, 2100, 8000, 4, 16, 570.1, 95.02",
        )
        .unwrap();
        assert_eq!(stats.name, "NVIDIA RTX Test");
        assert_eq!(stats.power_draw, 120.5);
        assert_eq!(stats.power_default, 250.0);
        assert_eq!(stats.pcie_width, "16");
    }

    #[test]
    fn unsupported_numeric_values_are_zero() {
        assert_eq!(parse_number("N/A"), 0.0);
    }

    #[test]
    fn parses_nvidia_settings_range() {
        let output = "  Attribute 'GPUTargetFanSpeed' ([fan:0]): 30.\n    The valid values for 'GPUTargetFanSpeed' are in the range 30 - 100 (inclusive).\n";
        assert_eq!(parse_setting_value(output), Some(30));
        assert_eq!(parse_setting_range(output), Some((30, 100)));
    }

    #[test]
    fn parses_negative_nvidia_settings_range() {
        let output = "  Attribute 'GPUGraphicsClockOffsetAllPerformanceLevels' ([gpu:0]): 0.\n    The valid values for 'GPUGraphicsClockOffsetAllPerformanceLevels' are in the range -1000 - 1000 (inclusive).\n";
        assert_eq!(parse_setting_range(output), Some((-1000, 1000)));
    }
}
