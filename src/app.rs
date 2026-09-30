use std::{
    collections::VecDeque,
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

use eframe::egui::{
    self, Color32, CornerRadius, ProgressBar, RichText, Sense, Stroke, StrokeKind, Vec2,
};

use crate::{
    gpu::{short_bus_id, GpuHistory, GpuStats, InventorySnapshot},
    nvidia::{Capability, GpuControls},
    worker::{ActionKind, Worker, WorkerRequest, WorkerResponse},
};

const ACCENT: Color32 = Color32::from_rgb(118, 185, 0);
const CYAN: Color32 = Color32::from_rgb(0, 180, 216);
const ORANGE: Color32 = Color32::from_rgb(255, 159, 28);
const RED: Color32 = Color32::from_rgb(230, 57, 70);
const MUTED: Color32 = Color32::from_rgb(140, 148, 164);
const CARD: Color32 = Color32::from_rgb(26, 30, 42);
const BORDER: Color32 = Color32::from_rgb(48, 54, 72);
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const CONTROL_RETRY_INTERVAL: Duration = Duration::from_secs(10);

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NVIDIA GPU Manager")
            .with_inner_size([1040.0, 760.0])
            .with_min_inner_size([820.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "NVIDIA GPU Manager",
        options,
        Box::new(|cc| Ok(Box::new(ManagerApp::new(cc)))),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Monitoring,
    Power,
    Fan,
    Tuning,
    Info,
}

#[derive(Clone)]
enum ControlsState {
    Unknown,
    Pending,
    Ready(GpuControls),
    Unavailable(String),
}

struct GpuState {
    stats: GpuStats,
    history: GpuHistory,
    controls: ControlsState,
    desired_power_limit: Option<u32>,
    fan_speed: Option<i32>,
    core_offset: Option<i32>,
    memory_offset: Option<i32>,
    last_control_probe: Instant,
}

impl GpuState {
    fn new(stats: GpuStats) -> Self {
        let desired_power_limit = stats.current_power_limit();
        let mut history = GpuHistory::default();
        history.push(&stats);
        Self {
            stats,
            history,
            controls: ControlsState::Unknown,
            desired_power_limit,
            fan_speed: None,
            core_offset: None,
            memory_offset: None,
            last_control_probe: Instant::now() - CONTROL_RETRY_INTERVAL,
        }
    }

    fn update_stats(&mut self, stats: GpuStats) {
        self.history.push(&stats);
        if self.desired_power_limit.is_none() {
            self.desired_power_limit = stats.current_power_limit();
        }
        self.stats = stats;
    }

    fn install_controls(&mut self, controls: GpuControls) {
        if self.fan_speed.is_none() {
            self.fan_speed = controls.fan.value().map(|fan| fan.range.current);
        }
        if self.core_offset.is_none() {
            self.core_offset = controls.core_offset.value().map(|range| range.current);
        }
        if self.memory_offset.is_none() {
            self.memory_offset = controls.memory_offset.value().map(|range| range.current);
        }
        self.controls = ControlsState::Ready(controls);
    }
}

struct ManagerApp {
    gpus: Vec<GpuState>,
    selected_uuid: Option<String>,
    selection_requires_user: bool,
    worker: Worker,
    poll_in_flight: bool,
    pending_action: Option<(String, ActionKind)>,
    last_poll: Instant,
    status: String,
    tab: Tab,
}

impl ManagerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        let mut app = Self {
            gpus: Vec::new(),
            selected_uuid: None,
            selection_requires_user: false,
            worker: Worker::spawn(),
            poll_in_flight: false,
            pending_action: None,
            last_poll: Instant::now() - POLL_INTERVAL,
            status: "Discovering NVIDIA GPUs...".to_owned(),
            tab: Tab::Monitoring,
        };
        app.request_poll();
        app
    }

    fn request_poll(&mut self) {
        if self.poll_in_flight {
            return;
        }
        match self.worker.requests.send(WorkerRequest::Poll) {
            Ok(()) => {
                self.poll_in_flight = true;
                self.last_poll = Instant::now();
            }
            Err(error) => self.status = format!("GPU worker unavailable: {error}"),
        }
    }

    fn drain_worker(&mut self) {
        loop {
            match self.worker.responses.try_recv() {
                Ok(response) => self.handle_response(response),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status = "GPU worker stopped unexpectedly".to_owned();
                    break;
                }
            }
        }
    }

    fn handle_response(&mut self, response: WorkerResponse) {
        match response {
            WorkerResponse::Poll(result) => {
                self.poll_in_flight = false;
                match result {
                    Ok(snapshot) => self.install_snapshot(snapshot),
                    Err(error) => {
                        self.status = format!("Monitoring temporarily unavailable: {error}")
                    }
                }
            }
            WorkerResponse::Controls { uuid, result } => {
                let Some(state) = self.gpus.iter_mut().find(|gpu| gpu.stats.id.uuid == uuid) else {
                    return;
                };
                match result {
                    Ok(controls) => state.install_controls(controls),
                    Err(error) => state.controls = ControlsState::Unavailable(error),
                }
            }
            WorkerResponse::Action { uuid, kind, result } => {
                if self
                    .pending_action
                    .as_ref()
                    .is_some_and(|pending| pending.0 == uuid && pending.1 == kind)
                {
                    self.pending_action = None;
                }
                self.status = match result {
                    Ok(message) => message,
                    Err(error) => format!("Change blocked or failed: {error}"),
                };
                if let Some(state) = self.gpus.iter_mut().find(|gpu| gpu.stats.id.uuid == uuid) {
                    state.controls = ControlsState::Unknown;
                }
                self.queue_control_probes();
                self.request_poll();
            }
        }
    }

    fn install_snapshot(&mut self, snapshot: InventorySnapshot) {
        let previous_selection = self.selected_uuid.clone();
        let mut previous = std::mem::take(&mut self.gpus);
        let mut next = Vec::with_capacity(snapshot.gpus.len());
        for stats in snapshot.gpus {
            if let Some(position) = previous
                .iter()
                .position(|state| state.stats.id.uuid == stats.id.uuid)
            {
                let mut state = previous.swap_remove(position);
                state.update_stats(stats);
                next.push(state);
            } else {
                next.push(GpuState::new(stats));
            }
        }
        next.sort_by_key(|state| state.stats.id.index);
        self.gpus = next;

        let (selection, selection_lost) = reconcile_selection(
            previous_selection.as_deref(),
            self.selection_requires_user,
            self.gpus.iter().map(|state| state.stats.id.uuid.as_str()),
        );
        self.selected_uuid = selection;
        if selection_lost {
            self.selection_requires_user = true;
        }

        if selection_lost {
            self.status = "The selected GPU disappeared; writes remain blocked until a new GPU is explicitly selected."
                .to_owned();
        } else if self.gpus.is_empty() {
            self.status = if snapshot.warnings.is_empty() {
                "No NVIDIA GPU detected".to_owned()
            } else {
                format!("No valid GPU data: {}", snapshot.warnings.join(" | "))
            };
        } else if snapshot.warnings.is_empty() {
            self.status = format!("Live monitoring: {} NVIDIA GPU(s)", self.gpus.len());
        } else {
            self.status = format!(
                "Live monitoring with {} parser warning(s): {}",
                snapshot.warnings.len(),
                snapshot.warnings.join(" | ")
            );
        }
        self.queue_control_probes();
    }

    fn queue_control_probes(&mut self) {
        for state in &mut self.gpus {
            let should_probe =
                control_probe_due(&state.controls, state.last_control_probe.elapsed());
            if !should_probe {
                continue;
            }
            let uuid = state.stats.id.uuid.clone();
            match self
                .worker
                .requests
                .send(WorkerRequest::ProbeControls { uuid })
            {
                Ok(()) => {
                    state.controls = ControlsState::Pending;
                    state.last_control_probe = Instant::now();
                }
                Err(error) => {
                    state.controls = ControlsState::Unavailable(format!(
                        "could not queue capability probe: {error}"
                    ))
                }
            }
        }
    }

    fn selected_index(&self) -> Option<usize> {
        let uuid = self.selected_uuid.as_ref()?;
        self.gpus
            .iter()
            .position(|state| state.stats.id.uuid == *uuid)
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(RichText::new("NVIDIA GPU Manager").color(ACCENT));
            ui.add_space(12.0);

            let selected_text = self
                .selected_index()
                .map(|index| self.gpus[index].stats.selector_label())
                .unwrap_or_else(|| "No GPU selected".to_owned());
            let mut selection = self.selected_uuid.clone();
            egui::ComboBox::from_id_salt("gpu-selector")
                .selected_text(selected_text)
                .width(360.0)
                .show_ui(ui, |ui| {
                    for state in &self.gpus {
                        ui.selectable_value(
                            &mut selection,
                            Some(state.stats.id.uuid.clone()),
                            state.stats.selector_label(),
                        );
                    }
                });
            if selection != self.selected_uuid {
                self.selected_uuid = selection;
                self.selection_requires_user = false;
                self.status = "GPU selection changed; controls apply only to this UUID.".to_owned();
            }
        });
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
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

    fn monitoring(&self, ui: &mut egui::Ui, selected: usize) {
        card(ui, "ALL GPUS", |ui| {
            egui::Grid::new("gpu-overview")
                .striped(true)
                .spacing([22.0, 7.0])
                .show(ui, |ui| {
                    for heading in ["GPU", "Temperature", "Load", "VRAM", "Power"] {
                        ui.colored_label(MUTED, RichText::new(heading).strong());
                    }
                    ui.end_row();
                    for state in &self.gpus {
                        let stats = &state.stats;
                        ui.label(format!(
                            "{} - {} - {}",
                            stats.id.index,
                            stats.name,
                            short_bus_id(&stats.id.pci_bus_id)
                        ));
                        ui.label(metric_text(stats.temp, " C", 0));
                        ui.label(metric_text(stats.gpu_util, "%", 0));
                        ui.label(memory_text(stats.mem_used, stats.mem_total));
                        ui.label(power_text(stats.power_draw, stats.power_limit));
                        ui.end_row();
                    }
                });
        });
        ui.add_space(12.0);

        let state = &self.gpus[selected];
        let stats = &state.stats;
        ui.columns(4, |columns| {
            stat_card(
                &mut columns[0],
                "TEMPERATURE",
                stats.temp,
                "C",
                RED,
                100.0,
                &state.history.temp,
            );
            stat_card(
                &mut columns[1],
                "GPU LOAD",
                stats.gpu_util,
                "%",
                ACCENT,
                100.0,
                &state.history.gpu_util,
            );
            stat_card(
                &mut columns[2],
                "POWER",
                stats.power_draw,
                "W",
                ORANGE,
                stats.power_max.unwrap_or(500.0).max(1.0),
                &state.history.power,
            );
            stat_card(
                &mut columns[3],
                "FAN",
                stats.fan,
                "%",
                CYAN,
                100.0,
                &state.history.fan,
            );
        });
        ui.add_space(12.0);
        card(ui, "MEMORY & LOAD", |ui| {
            metric_bar(
                ui,
                "GPU",
                ratio(stats.gpu_util, Some(100.0)),
                metric_text(stats.gpu_util, "%", 0),
            );
            metric_bar(
                ui,
                "VRAM",
                ratio(stats.mem_used, stats.mem_total),
                memory_text(stats.mem_used, stats.mem_total),
            );
            metric_bar(
                ui,
                "Memory load",
                ratio(stats.mem_util, Some(100.0)),
                metric_text(stats.mem_util, "%", 0),
            );
            metric_bar(
                ui,
                "Temp",
                ratio(stats.temp, Some(100.0)),
                metric_text(stats.temp, " C", 0),
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
                    ui.colored_label(ACCENT, metric_text(stats.clk_gpu, " MHz", 0));
                    ui.label(metric_text(stats.clk_gpu_max, " MHz", 0));
                    ui.end_row();
                    ui.label("Memory");
                    ui.colored_label(CYAN, metric_text(stats.clk_mem, " MHz", 0));
                    ui.label(metric_text(stats.clk_mem_max, " MHz", 0));
                    ui.end_row();
                });
        });
    }

    fn power(&mut self, ui: &mut egui::Ui, selected: usize) {
        let stats = self.gpus[selected].stats.clone();
        card(ui, "CURRENT POWER USAGE", |ui| {
            ui.colored_label(
                ORANGE,
                RichText::new(metric_text(stats.power_draw, " W", 1)).size(25.0),
            );
            ui.label(format!(
                "Current limit: {}",
                metric_text(stats.power_limit, " W", 0)
            ));
            ui.colored_label(
                MUTED,
                match stats.power_range() {
                    Some((min, max)) => format!("Driver range: {min}-{max} W"),
                    None => "Driver range: unavailable".to_owned(),
                },
            );
        });
        ui.add_space(12.0);

        let busy = self.pending_action.is_some();
        let mut action = None;
        card(ui, "SET POWER LIMIT", |ui| {
            let Some((min, max)) = stats.power_range() else {
                ui.colored_label(
                    RED,
                    "Power control is disabled because the driver did not report a valid range.",
                );
                return;
            };
            let desired = self.gpus[selected]
                .desired_power_limit
                .get_or_insert_with(|| stats.current_power_limit().unwrap_or(min));
            *desired = (*desired).clamp(min, max);
            ui.colored_label(ORANGE, RichText::new(format!("{desired} W")).size(23.0));
            ui.add(egui::Slider::new(desired, min..=max));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("Apply limit"))
                    .clicked()
                {
                    action = Some(*desired);
                }
                if ui
                    .add_enabled(
                        !busy && stats.default_power_limit().is_some(),
                        egui::Button::new("Reset to default"),
                    )
                    .clicked()
                {
                    if let Some(default) = stats.default_power_limit() {
                        *desired = default;
                        action = Some(default);
                    }
                }
            });
            ui.colored_label(
                MUTED,
                "The UUID and driver range are revalidated before Polkit is invoked.",
            );
        });
        if let Some(watts) = action {
            let uuid = stats.id.uuid;
            self.send_action(
                uuid.clone(),
                ActionKind::Power,
                WorkerRequest::SetPower { uuid, watts },
            );
        }
    }

    fn fan(&mut self, ui: &mut egui::Ui, selected: usize) {
        let stats = self.gpus[selected].stats.clone();
        card(ui, "FAN STATUS", |ui| {
            ui.colored_label(
                CYAN,
                RichText::new(metric_text(stats.fan, "%", 0)).size(25.0),
            );
        });
        ui.add_space(12.0);

        let controls = self.gpus[selected].controls.clone();
        let busy = self.pending_action.is_some();
        let mut action: Option<Option<i32>> = None;
        card(ui, "MANUAL FAN CONTROL", |ui| match controls {
            ControlsState::Unknown | ControlsState::Pending => {
                ui.colored_label(MUTED, "Checking driver fan mapping...");
                if ui
                    .add_enabled(!busy, egui::Button::new("Restore automatic control"))
                    .clicked()
                {
                    action = Some(None);
                }
            }
            ControlsState::Unavailable(error) => {
                ui.colored_label(RED, format!("Fan control unavailable: {error}"));
                if ui
                    .add_enabled(!busy, egui::Button::new("Restore automatic control"))
                    .clicked()
                {
                    action = Some(None);
                }
            }
            ControlsState::Ready(controls) => match controls.fan {
                Capability::Supported(fan) => {
                    let desired = self.gpus[selected]
                        .fan_speed
                        .get_or_insert(fan.range.current);
                    *desired = (*desired).clamp(fan.range.min, fan.range.max);
                    ui.label(if fan.manual {
                        "Mode: manual"
                    } else {
                        "Mode: automatic"
                    });
                    ui.colored_label(CYAN, RichText::new(format!("{desired}%")).size(23.0));
                    ui.add(egui::Slider::new(desired, fan.range.min..=fan.range.max));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!busy, egui::Button::new("Apply fan speed"))
                            .clicked()
                        {
                            action = Some(Some(*desired));
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new("Automatic"))
                            .clicked()
                        {
                            action = Some(None);
                        }
                    });
                    ui.colored_label(
                        MUTED,
                        format!(
                            "Verified NV-CONTROL mapping: {} fan target(s), common range {}-{}%.",
                            fan.targets.len(),
                            fan.range.min,
                            fan.range.max
                        ),
                    );
                    ui.colored_label(
                        MUTED,
                        "Requires nvidia-settings, an X control display, and Coolbits fan support.",
                    );
                }
                capability => {
                    let label = if capability.is_retryable() {
                        "Fan mapping temporarily unavailable"
                    } else {
                        "Manual fan control unsupported"
                    };
                    ui.colored_label(
                        if capability.is_retryable() {
                            ORANGE
                        } else {
                            RED
                        },
                        format!(
                            "{label}: {}",
                            capability.reason().unwrap_or("no driver details")
                        ),
                    );
                    if ui
                        .add_enabled(!busy, egui::Button::new("Restore automatic control"))
                        .clicked()
                    {
                        action = Some(None);
                    }
                    ui.colored_label(
                        MUTED,
                        "Automatic recovery addresses only the verified GPU and does not require a speed mapping.",
                    );
                }
            },
        });
        if let Some(speed) = action {
            self.send_action(
                stats.id.uuid.clone(),
                ActionKind::Fan,
                WorkerRequest::SetFan {
                    uuid: stats.id.uuid,
                    speed,
                },
            );
        }
    }

    fn tuning(&mut self, ui: &mut egui::Ui, selected: usize) {
        let uuid = self.gpus[selected].stats.id.uuid.clone();
        let controls = self.gpus[selected].controls.clone();
        let (core_supported, memory_supported) = match &controls {
            ControlsState::Ready(controls) => (
                controls.core_offset.value().is_some(),
                controls.memory_offset.value().is_some(),
            ),
            _ => (false, false),
        };
        let busy = self.pending_action.is_some();
        let mut apply = false;
        let mut reset = false;
        card(ui, "GPU-SPECIFIC TUNING", |ui| {
            ui.colored_label(
                ORANGE,
                "Clock offsets can make the system unstable. Increase values gradually.",
            );
            ui.add_space(8.0);
            match controls {
                ControlsState::Unknown | ControlsState::Pending => {
                    ui.colored_label(MUTED, "Checking UUID-scoped tuning capabilities...");
                }
                ControlsState::Unavailable(error) => {
                    ui.colored_label(RED, format!("Tuning unavailable: {error}"));
                }
                ControlsState::Ready(controls) => {
                    match &controls.core_offset {
                        Capability::Supported(range) => {
                            let value =
                                self.gpus[selected].core_offset.get_or_insert(range.current);
                            *value = (*value).clamp(range.min, range.max);
                            ui.label(format!("GPU core offset: {value:+} MHz"));
                            ui.add(egui::Slider::new(value, range.min..=range.max));
                            ui.colored_label(
                                MUTED,
                                format!("Driver range: {:+} to {:+} MHz", range.min, range.max),
                            );
                        }
                        capability => {
                            ui.colored_label(
                                if capability.is_retryable() {
                                    ORANGE
                                } else {
                                    MUTED
                                },
                                capability
                                    .reason()
                                    .unwrap_or("Core clock offsets unavailable"),
                            );
                        }
                    }
                    ui.add_space(8.0);
                    match &controls.memory_offset {
                        Capability::Supported(range) => {
                            let value = self.gpus[selected]
                                .memory_offset
                                .get_or_insert(range.current);
                            *value = (*value).clamp(range.min, range.max);
                            ui.label(format!("Memory transfer offset: {value:+} MHz"));
                            ui.add(egui::Slider::new(value, range.min..=range.max));
                            ui.colored_label(
                                MUTED,
                                format!("Driver range: {:+} to {:+} MHz", range.min, range.max),
                            );
                        }
                        capability => {
                            ui.colored_label(
                                if capability.is_retryable() {
                                    ORANGE
                                } else {
                                    MUTED
                                },
                                capability
                                    .reason()
                                    .unwrap_or("Memory transfer offsets unavailable"),
                            );
                        }
                    }
                    ui.add_space(10.0);
                    let supported = controls.core_offset.value().is_some()
                        || controls.memory_offset.value().is_some();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!busy && supported, egui::Button::new("Apply offsets"))
                            .clicked()
                        {
                            apply = true;
                        }
                        if ui
                            .add_enabled(!busy && supported, egui::Button::new("Reset offsets"))
                            .clicked()
                        {
                            reset = true;
                        }
                    });
                    ui.colored_label(
                        MUTED,
                        "Targets are addressed by verified GPU UUID and normally require Coolbits=8.",
                    );
                }
            }
        });

        if reset {
            if core_supported {
                self.gpus[selected].core_offset = Some(0);
            }
            if memory_supported {
                self.gpus[selected].memory_offset = Some(0);
            }
            apply = true;
        }
        if apply {
            self.send_action(
                uuid.clone(),
                ActionKind::Tuning,
                WorkerRequest::SetTuning {
                    uuid,
                    core_offset: core_supported
                        .then_some(self.gpus[selected].core_offset)
                        .flatten(),
                    memory_offset: memory_supported
                        .then_some(self.gpus[selected].memory_offset)
                        .flatten(),
                },
            );
        }
    }

    fn info(&self, ui: &mut egui::Ui, selected: usize) {
        let stats = &self.gpus[selected].stats;
        card(ui, "GPU INFORMATION", |ui| {
            egui::Grid::new("info").spacing([28.0, 8.0]).show(ui, |ui| {
                for (label, value) in [
                    ("GPU model", stats.name.clone()),
                    ("Current index", stats.id.index.to_string()),
                    ("Stable UUID", stats.id.uuid.clone()),
                    ("PCI bus", stats.id.pci_bus_id.clone()),
                    ("Driver", option_text(stats.driver.as_deref())),
                    ("VRAM total", metric_text(stats.mem_total, " MiB", 0)),
                    (
                        "PCIe generation",
                        stats
                            .pcie_gen
                            .as_ref()
                            .map_or_else(|| "N/A".to_owned(), |value| format!("Gen {value}")),
                    ),
                    (
                        "PCIe width",
                        stats
                            .pcie_width
                            .as_ref()
                            .map_or_else(|| "N/A".to_owned(), |value| format!("x{value}")),
                    ),
                    ("VBIOS version", option_text(stats.vbios.as_deref())),
                ] {
                    ui.colored_label(MUTED, label);
                    ui.label(value);
                    ui.end_row();
                }
            });
        });
        ui.add_space(12.0);
        card(ui, "SECURITY MODEL", |ui| {
            ui.label("Monitoring is unprivileged. Only power-limit changes invoke Polkit.");
            ui.label("Every write is revalidated against a fresh UUID-scoped driver snapshot.");
            ui.colored_label(
                MUTED,
                "Fan and tuning controls fail closed when identity, mapping, capability, or range cannot be proven.",
            );
        });
    }

    fn send_action(&mut self, uuid: String, kind: ActionKind, request: WorkerRequest) {
        if self.pending_action.is_some() {
            self.status = "Another GPU change is still in progress".to_owned();
            return;
        }
        match self.worker.requests.send(request) {
            Ok(()) => {
                self.pending_action = Some((uuid, kind));
                self.status = "Validating the selected GPU and requested change...".to_owned();
            }
            Err(error) => self.status = format!("Could not queue GPU change: {error}"),
        }
    }
}

impl eframe::App for ManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_worker();
        if self.last_poll.elapsed() >= POLL_INTERVAL {
            self.request_poll();
        }
        ctx.request_repaint_after(Duration::from_millis(200));

        egui::TopBottomPanel::bottom("status-bar").show(ctx, |ui| {
            ui.separator();
            ui.colored_label(MUTED, &self.status);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.top_bar(ui);
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("tab-content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(selected) = self.selected_index() {
                        match self.tab {
                            Tab::Monitoring => self.monitoring(ui, selected),
                            Tab::Power => self.power(ui, selected),
                            Tab::Fan => self.fan(ui, selected),
                            Tab::Tuning => self.tuning(ui, selected),
                            Tab::Info => self.info(ui, selected),
                        }
                    } else {
                        ui.colored_label(RED, "No NVIDIA GPU data available.");
                        ui.label(
                            "Monitoring will retry automatically. Check that the NVIDIA driver and nvidia-smi are available.",
                        );
                    }
                });
        });
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(13, 15, 20);
    visuals.window_fill = CARD;
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.selection.bg_fill = ACCENT;
    ctx.set_visuals(visuals);
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
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
    value: Option<f32>,
    unit: &str,
    color: Color32,
    max: f32,
    history: &VecDeque<Option<f32>>,
) {
    card(ui, title, |ui| {
        ui.colored_label(
            color,
            RichText::new(metric_text(value, &format!(" {unit}"), 0))
                .size(24.0)
                .strong(),
        );
        sparkline(ui, history, max, color);
    });
}

fn sparkline(ui: &mut egui::Ui, values: &VecDeque<Option<f32>>, max: f32, color: Color32) {
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 62.0), Sense::hover());
    let rect = response.rect;
    painter.rect(
        rect,
        CornerRadius::same(4),
        CARD,
        Stroke::new(1.0_f32, BORDER),
        StrokeKind::Inside,
    );
    if values.len() < 2 || max <= 0.0 {
        return;
    }

    let mut segment = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if let Some(value) = value {
            let x = rect.left() + rect.width() * index as f32 / (values.len() - 1) as f32;
            let y = rect.bottom() - rect.height() * (value / max).clamp(0.0, 1.0);
            segment.push(egui::pos2(x, y));
        } else if segment.len() >= 2 {
            painter.add(egui::Shape::line(
                std::mem::take(&mut segment),
                Stroke::new(2.0_f32, color),
            ));
        } else {
            segment.clear();
        }
    }
    if segment.len() >= 2 {
        painter.add(egui::Shape::line(segment, Stroke::new(2.0_f32, color)));
    }
}

fn metric_bar(ui: &mut egui::Ui, label: &str, value: Option<f32>, text: String) {
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.label(format!("{label}:"));
        ui.add(ProgressBar::new(value.unwrap_or(0.0).clamp(0.0, 1.0)).text(text));
    });
}

fn ratio(value: Option<f32>, maximum: Option<f32>) -> Option<f32> {
    let value = value?;
    let maximum = maximum?;
    (maximum > 0.0).then_some(value / maximum)
}

fn metric_text(value: Option<f32>, suffix: &str, decimals: usize) -> String {
    value.map_or_else(
        || "N/A".to_owned(),
        |value| format!("{value:.decimals$}{suffix}"),
    )
}

fn memory_text(used: Option<f32>, total: Option<f32>) -> String {
    match (used, total) {
        (Some(used), Some(total)) => format!("{used:.0} / {total:.0} MiB"),
        _ => "N/A".to_owned(),
    }
}

fn power_text(draw: Option<f32>, limit: Option<f32>) -> String {
    match (draw, limit) {
        (Some(draw), Some(limit)) => format!("{draw:.0} / {limit:.0} W"),
        (Some(draw), None) => format!("{draw:.0} W / N/A"),
        _ => "N/A".to_owned(),
    }
}

fn option_text(value: Option<&str>) -> String {
    value.unwrap_or("N/A").to_owned()
}

fn control_probe_due(controls: &ControlsState, elapsed: Duration) -> bool {
    match controls {
        ControlsState::Unknown => true,
        ControlsState::Unavailable(_) => elapsed >= CONTROL_RETRY_INTERVAL,
        ControlsState::Ready(controls) => {
            controls.needs_retry() && elapsed >= CONTROL_RETRY_INTERVAL
        }
        ControlsState::Pending => false,
    }
}

fn reconcile_selection<'a>(
    previous: Option<&str>,
    selection_requires_user: bool,
    uuids: impl Iterator<Item = &'a str>,
) -> (Option<String>, bool) {
    let uuids: Vec<_> = uuids.collect();
    match previous {
        Some(uuid) if uuids.contains(&uuid) => (Some(uuid.to_owned()), false),
        Some(_) => (None, true),
        None if selection_requires_user => (None, false),
        None => (uuids.first().map(|uuid| (*uuid).to_owned()), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::InventorySnapshot;

    const GPU_A: &str = "0, GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee, 00000000:01:00.0, GPU A, 50, 20, 10, 100, 1000, 30, 50, 100, 80, 120, 100, 500, 1000, 1000, 2000, 4, 16, 615, vbios";
    const GPU_B: &str = "1, GPU-11111111-2222-3333-4444-555555555555, 00000000:09:00.0, GPU B, 40, 10, 5, 200, 2000, 40, 60, 110, 90, 130, 110, 600, 1100, 1100, 2100, 4, 8, 615, vbios";

    #[test]
    fn gpu_state_keeps_per_gpu_history() {
        let snapshot = InventorySnapshot::parse_csv(&format!("{GPU_A}\n{GPU_B}"));
        let a = GpuState::new(snapshot.gpus[0].clone());
        let b = GpuState::new(snapshot.gpus[1].clone());
        assert_eq!(a.history.temp.back(), Some(&Some(50.0)));
        assert_eq!(b.history.temp.back(), Some(&Some(40.0)));
        assert_ne!(a.stats.id.uuid, b.stats.id.uuid);
    }

    #[test]
    fn power_limit_selection_is_isolated_per_gpu() {
        let snapshot = InventorySnapshot::parse_csv(&format!("{GPU_A}\n{GPU_B}"));
        let mut states: Vec<_> = snapshot.gpus.into_iter().map(GpuState::new).collect();

        states[0].desired_power_limit = Some(115);

        assert_eq!(states[0].desired_power_limit, Some(115));
        assert_eq!(states[1].desired_power_limit, Some(110));
        assert_ne!(states[0].stats.id.uuid, states[1].stats.id.uuid);
    }

    #[test]
    fn missing_metrics_render_as_na() {
        assert_eq!(metric_text(None, " W", 0), "N/A");
        assert_eq!(memory_text(Some(1.0), None), "N/A");
    }

    #[test]
    fn disappearing_selection_fails_closed() {
        let remaining = ["GPU-11111111-2222-3333-4444-555555555555"];
        let (selection, lost) = reconcile_selection(
            Some("GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
            false,
            remaining.into_iter(),
        );
        assert_eq!(selection, None);
        assert!(lost);
    }

    #[test]
    fn stable_selection_survives_index_changes() {
        let selected = "GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let uuids = ["GPU-11111111-2222-3333-4444-555555555555", selected];
        let (selection, lost) = reconcile_selection(Some(selected), false, uuids.into_iter());
        assert_eq!(selection.as_deref(), Some(selected));
        assert!(!lost);
    }

    #[test]
    fn hotplug_lock_requires_explicit_reselection() {
        let uuids = ["GPU-11111111-2222-3333-4444-555555555555"];
        let (selection, lost) = reconcile_selection(None, true, uuids.into_iter());
        assert_eq!(selection, None);
        assert!(!lost);
    }

    #[test]
    fn retryable_partial_capability_probe_is_queued_again() {
        let retryable = ControlsState::Ready(GpuControls {
            fan: Capability::Retryable("temporary X failure".to_owned()),
            core_offset: Capability::Unsupported("not exposed".to_owned()),
            memory_offset: Capability::Unsupported("not exposed".to_owned()),
        });
        assert!(!control_probe_due(
            &retryable,
            CONTROL_RETRY_INTERVAL - Duration::from_millis(1)
        ));
        assert!(control_probe_due(&retryable, CONTROL_RETRY_INTERVAL));

        let unsupported = ControlsState::Ready(GpuControls {
            fan: Capability::Unsupported("not exposed".to_owned()),
            core_offset: Capability::Unsupported("not exposed".to_owned()),
            memory_offset: Capability::Unsupported("not exposed".to_owned()),
        });
        assert!(!control_probe_due(&unsupported, CONTROL_RETRY_INTERVAL * 2));
    }
}
