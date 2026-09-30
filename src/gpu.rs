use std::collections::VecDeque;

pub const QUERY: &str = "index,uuid,pci.bus_id,name,temperature.gpu,utilization.gpu,utilization.memory,memory.used,memory.total,fan.speed,power.draw,power.limit,power.min_limit,power.max_limit,power.default_limit,clocks.gr,clocks.mem,clocks.max.gr,clocks.max.mem,pcie.link.gen.current,pcie.link.width.current,driver_version,vbios_version";
const FIELD_COUNT: usize = 23;
const HISTORY_LENGTH: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuId {
    pub index: u32,
    pub uuid: String,
    pub pci_bus_id: String,
}

#[derive(Clone, Debug)]
pub struct GpuStats {
    pub id: GpuId,
    pub name: String,
    pub temp: Option<f32>,
    pub gpu_util: Option<f32>,
    pub mem_util: Option<f32>,
    pub mem_used: Option<f32>,
    pub mem_total: Option<f32>,
    pub fan: Option<f32>,
    pub power_draw: Option<f32>,
    pub power_limit: Option<f32>,
    pub power_min: Option<f32>,
    pub power_max: Option<f32>,
    pub power_default: Option<f32>,
    pub clk_gpu: Option<f32>,
    pub clk_mem: Option<f32>,
    pub clk_gpu_max: Option<f32>,
    pub clk_mem_max: Option<f32>,
    pub pcie_gen: Option<String>,
    pub pcie_width: Option<String>,
    pub driver: Option<String>,
    pub vbios: Option<String>,
}

impl GpuStats {
    pub fn power_range(&self) -> Option<(u32, u32)> {
        let min = self.power_min?.ceil();
        let max = self.power_max?.floor();
        if !min.is_finite()
            || !max.is_finite()
            || min < 1.0
            || max < min
            || f64::from(max) > f64::from(u32::MAX)
        {
            return None;
        }
        Some((min as u32, max as u32))
    }

    pub fn current_power_limit(&self) -> Option<u32> {
        rounded_in_range(self.power_limit?, self.power_range()?)
    }

    pub fn default_power_limit(&self) -> Option<u32> {
        rounded_in_range(self.power_default?, self.power_range()?)
    }

    pub fn selector_label(&self) -> String {
        format!(
            "GPU {} - {} - {}",
            self.id.index,
            self.name,
            short_bus_id(&self.id.pci_bus_id)
        )
    }
}

fn rounded_in_range(value: f32, range: (u32, u32)) -> Option<u32> {
    if !value.is_finite() || value < 0.0 || f64::from(value) > f64::from(u32::MAX) {
        return None;
    }
    let rounded = value.round() as u32;
    (range.0..=range.1).contains(&rounded).then_some(rounded)
}

#[derive(Clone, Debug, Default)]
pub struct InventorySnapshot {
    pub gpus: Vec<GpuStats>,
    pub warnings: Vec<String>,
}

impl InventorySnapshot {
    pub fn parse_csv(raw: &str) -> Self {
        let mut snapshot = Self::default();
        for (line_index, line) in raw.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match parse_gpu_line(line, line_index + 1, &mut snapshot.warnings) {
                Ok(gpu) => snapshot.gpus.push(gpu),
                Err(error) => snapshot.warnings.push(error),
            }
        }
        snapshot
    }
}

fn parse_gpu_line(
    line: &str,
    line_number: usize,
    warnings: &mut Vec<String>,
) -> Result<GpuStats, String> {
    let values: Vec<_> = line.split(',').map(str::trim).collect();
    if values.len() != FIELD_COUNT {
        return Err(format!(
            "line {line_number}: expected {FIELD_COUNT} fields, received {}",
            values.len()
        ));
    }

    let index = values[0]
        .parse::<u32>()
        .map_err(|_| format!("line {line_number}: invalid GPU index"))?;
    let uuid = values[1];
    if !valid_gpu_uuid(uuid) {
        return Err(format!("line {line_number}: invalid GPU UUID"));
    }
    let pci_bus_id = values[2];
    if !valid_pci_bus_id(pci_bus_id) {
        return Err(format!("line {line_number}: invalid PCI bus ID"));
    }
    let name = values[3];
    if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
        return Err(format!("line {line_number}: invalid GPU name"));
    }

    let mut metric = |field: usize, label: &str| {
        parse_optional_number(values[field]).unwrap_or_else(|error| {
            warnings.push(format!(
                "GPU {index} ({uuid}), {label}: {error}; value ignored"
            ));
            None
        })
    };

    Ok(GpuStats {
        id: GpuId {
            index,
            uuid: uuid.to_owned(),
            pci_bus_id: pci_bus_id.to_owned(),
        },
        name: name.to_owned(),
        temp: metric(4, "temperature"),
        gpu_util: metric(5, "GPU utilization"),
        mem_util: metric(6, "memory utilization"),
        mem_used: metric(7, "used memory"),
        mem_total: metric(8, "total memory"),
        fan: metric(9, "fan speed"),
        power_draw: metric(10, "power draw"),
        power_limit: metric(11, "power limit"),
        power_min: metric(12, "minimum power limit"),
        power_max: metric(13, "maximum power limit"),
        power_default: metric(14, "default power limit"),
        clk_gpu: metric(15, "graphics clock"),
        clk_mem: metric(16, "memory clock"),
        clk_gpu_max: metric(17, "maximum graphics clock"),
        clk_mem_max: metric(18, "maximum memory clock"),
        pcie_gen: optional_text(values[19]),
        pcie_width: optional_text(values[20]),
        driver: optional_text(values[21]),
        vbios: optional_text(values[22]),
    })
}

fn parse_optional_number(value: &str) -> Result<Option<f32>, String> {
    if is_unavailable(value) {
        return Ok(None);
    }
    let number = value
        .parse::<f32>()
        .map_err(|_| format!("invalid number '{value}'"))?;
    if !number.is_finite() {
        return Err(format!("non-finite number '{value}'"));
    }
    Ok(Some(number))
}

fn optional_text(value: &str) -> Option<String> {
    (!is_unavailable(value) && !value.is_empty()).then(|| value.to_owned())
}

fn is_unavailable(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "N/A" | "[N/A]" | "NOT SUPPORTED" | "NOT AVAILABLE"
    )
}

pub fn valid_gpu_uuid(value: &str) -> bool {
    let Some(identifier) = value.strip_prefix("GPU-") else {
        return false;
    };
    (8..=80).contains(&identifier.len())
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

fn valid_pci_bus_id(value: &str) -> bool {
    (7..=32).contains(&value.len())
        && value.contains(':')
        && value.contains('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || matches!(byte, b':' | b'.'))
}

pub fn short_bus_id(value: &str) -> &str {
    value
        .strip_prefix("00000000:")
        .or_else(|| value.strip_prefix("0000:"))
        .unwrap_or(value)
}

#[derive(Clone, Debug)]
pub struct GpuHistory {
    pub temp: VecDeque<Option<f32>>,
    pub gpu_util: VecDeque<Option<f32>>,
    pub power: VecDeque<Option<f32>>,
    pub fan: VecDeque<Option<f32>>,
}

impl Default for GpuHistory {
    fn default() -> Self {
        Self {
            temp: empty_history(),
            gpu_util: empty_history(),
            power: empty_history(),
            fan: empty_history(),
        }
    }
}

impl GpuHistory {
    pub fn push(&mut self, stats: &GpuStats) {
        push_value(&mut self.temp, stats.temp);
        push_value(&mut self.gpu_util, stats.gpu_util);
        push_value(&mut self.power, stats.power_draw);
        push_value(&mut self.fan, stats.fan);
    }
}

fn empty_history() -> VecDeque<Option<f32>> {
    VecDeque::from(vec![None; HISTORY_LENGTH])
}

fn push_value(history: &mut VecDeque<Option<f32>>, value: Option<f32>) {
    if history.len() == HISTORY_LENGTH {
        history.pop_front();
    }
    history.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPU_0: &str = "0, GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee, 00000000:01:00.0, NVIDIA RTX 4090, 55, 71, 12, 1024, 24564, 44, 120.5, 250, 100, 300, 250, 1800, 7000, 2100, 8000, 4, 16, 615.1, 95.02";
    const GPU_1: &str = "1, GPU-11111111-2222-3333-4444-555555555555, 00000000:09:00.0, NVIDIA RTX A4000, 42, 10, 2, 512, 16376, 41, 60, 140, 100, 140, 140, 900, 6000, 1560, 7000, 4, 8, 615.1, 94.04";
    const GPU_2: &str = "2, GPU-66666666-7777-8888-9999-aaaaaaaaaaaa, 00000000:0b:00.0, NVIDIA RTX A4000, 43, 11, 3, 768, 16376, 42, 62, 140, 100, 140, 140, 910, 6010, 1560, 7000, 4, 8, 615.1, 94.04";

    #[test]
    fn parses_multiple_gpus() {
        let snapshot = InventorySnapshot::parse_csv(&format!("{GPU_0}\n{GPU_1}\n{GPU_2}\n"));
        assert_eq!(snapshot.gpus.len(), 3);
        assert!(snapshot.warnings.is_empty());
        assert_eq!(snapshot.gpus[1].id.index, 1);
        assert_eq!(snapshot.gpus[2].name, "NVIDIA RTX A4000");
    }

    #[test]
    fn empty_output_is_a_valid_empty_inventory() {
        let snapshot = InventorySnapshot::parse_csv("\n\n");
        assert!(snapshot.gpus.is_empty());
        assert!(snapshot.warnings.is_empty());
    }

    #[test]
    fn identical_models_remain_distinct() {
        let snapshot = InventorySnapshot::parse_csv(&format!("{GPU_1}\n{GPU_2}"));
        assert_ne!(snapshot.gpus[0].id.uuid, snapshot.gpus[1].id.uuid);
        assert_ne!(
            snapshot.gpus[0].id.pci_bus_id,
            snapshot.gpus[1].id.pci_bus_id
        );
        assert_ne!(
            snapshot.gpus[0].selector_label(),
            snapshot.gpus[1].selector_label()
        );
    }

    #[test]
    fn unavailable_values_are_not_zero() {
        let line = GPU_0.replacen(", 55,", ", N/A,", 1);
        let snapshot = InventorySnapshot::parse_csv(&line);
        assert_eq!(snapshot.gpus.len(), 1);
        assert_eq!(snapshot.gpus[0].temp, None);
    }

    #[test]
    fn invalid_float_is_isolated_to_its_metric() {
        let line = GPU_0.replacen(", 55,", ", hot,", 1);
        let snapshot = InventorySnapshot::parse_csv(&format!("{line}\n{GPU_1}"));
        assert_eq!(snapshot.gpus.len(), 2);
        assert_eq!(snapshot.gpus[0].temp, None);
        assert_eq!(snapshot.warnings.len(), 1);
    }

    #[test]
    fn malformed_lines_do_not_hide_valid_gpus() {
        let input = format!("{GPU_0}\nmissing,columns\n{GPU_1},extra\n\n{GPU_2}");
        let snapshot = InventorySnapshot::parse_csv(&input);
        assert_eq!(snapshot.gpus.len(), 2);
        assert_eq!(snapshot.warnings.len(), 2);
    }

    #[test]
    fn invalid_index_and_uuid_are_rejected() {
        let invalid_index = GPU_0.replacen("0,", "zero,", 1);
        let invalid_uuid = GPU_1.replacen("GPU-11111111", "GPU-$(bad)", 1);
        let snapshot = InventorySnapshot::parse_csv(&format!("{invalid_index}\n{invalid_uuid}"));
        assert!(snapshot.gpus.is_empty());
        assert_eq!(snapshot.warnings.len(), 2);
    }

    #[test]
    fn power_range_rounds_inward_and_validates_defaults() {
        let snapshot = InventorySnapshot::parse_csv(GPU_0);
        let stats = &snapshot.gpus[0];
        assert_eq!(stats.power_range(), Some((100, 300)));
        assert_eq!(stats.current_power_limit(), Some(250));
        assert_eq!(stats.default_power_limit(), Some(250));
    }

    #[test]
    fn power_range_rejects_integer_overflow() {
        let line = GPU_0.replacen(", 300,", ", 4294967296,", 1);
        let snapshot = InventorySnapshot::parse_csv(&line);
        assert_eq!(snapshot.gpus[0].power_range(), None);
    }

    #[test]
    fn non_finite_values_are_rejected() {
        for value in ["NaN", "inf", "-inf"] {
            assert!(parse_optional_number(value).is_err());
        }
    }

    #[test]
    fn validates_gpu_uuid_for_command_arguments() {
        assert!(valid_gpu_uuid("GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"));
        assert!(!valid_gpu_uuid("GPU-0;touch /tmp/x"));
        assert!(!valid_gpu_uuid("[gpu:0]"));
        assert!(!valid_gpu_uuid("MIG-aaaaaaaa-bbbb"));
    }

    #[test]
    fn shortens_domain_qualified_bus_id() {
        assert_eq!(short_bus_id("00000000:09:00.0"), "09:00.0");
    }
}
