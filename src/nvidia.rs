use std::{collections::HashSet, path::Path, process::Command};

use crate::gpu::{valid_gpu_uuid, GpuStats, InventorySnapshot, QUERY};

const GPU_FAN_CONTROL_STATE: &str = "GPUFanControlState";
const GPU_TARGET_FAN_SPEED: &str = "GPUTargetFanSpeed";
const CORE_OFFSET: &str = "GPUGraphicsClockOffsetAllPerformanceLevels";
const MEMORY_OFFSET: &str = "GPUMemoryTransferRateOffsetAllPerformanceLevels";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingRange {
    pub current: i32,
    pub min: i32,
    pub max: i32,
}

impl SettingRange {
    pub fn contains(&self, value: i32) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

#[derive(Clone, Debug)]
pub struct FanControl {
    pub range: SettingRange,
    pub manual: bool,
    pub targets: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct GpuControls {
    pub fan: Option<FanControl>,
    pub fan_reason: String,
    pub core_offset: Option<SettingRange>,
    pub core_reason: String,
    pub memory_offset: Option<SettingRange>,
    pub memory_reason: String,
}

#[derive(Clone, Debug)]
struct SettingInstance {
    target_id: u32,
    range: SettingRange,
}

pub fn read_inventory() -> Result<InventorySnapshot, String> {
    let binary = resolve_executable(
        "nvidia-smi",
        &[
            "/run/current-system/sw/bin/nvidia-smi",
            "/run/opengl-driver/bin/nvidia-smi",
        ],
    )?;
    let output = Command::new(binary)
        .args([
            format!("--query-gpu={QUERY}"),
            "--format=csv,noheader,nounits".to_owned(),
        ])
        .output()
        .map_err(|error| format!("could not start {binary}: {error}"))?;
    if !output.status.success() {
        return Err(command_failure("nvidia-smi", &output));
    }
    Ok(InventorySnapshot::parse_csv(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

pub fn probe_controls(uuid: &str) -> Result<GpuControls, String> {
    verify_settings_gpu(uuid)?;

    let gpu_fan_target = gpu_target(uuid, GPU_FAN_CONTROL_STATE)?;
    let fan_target = fan_qualifier_target(uuid, GPU_TARGET_FAN_SPEED)?;
    let fan = match (
        query_setting_value(&gpu_fan_target),
        query_setting_instances(&fan_target, "fan"),
    ) {
        (Ok(manual), Ok(instances)) => build_fan_control(manual != 0, instances).ok(),
        _ => None,
    };
    let fan_reason = if fan.is_some() {
        String::new()
    } else {
        "The driver did not provide a verified GPU-to-fan relationship and writable range."
            .to_owned()
    };

    let core_target = gpu_target(uuid, CORE_OFFSET)?;
    let (core_offset, core_reason) = optional_gpu_range(&core_target, "Core clock offsets");
    let memory_target = gpu_target(uuid, MEMORY_OFFSET)?;
    let (memory_offset, memory_reason) =
        optional_gpu_range(&memory_target, "Memory transfer offsets");

    Ok(GpuControls {
        fan,
        fan_reason,
        core_offset,
        core_reason,
        memory_offset,
        memory_reason,
    })
}

pub fn set_power_limit(uuid: &str, watts: u32) -> Result<String, String> {
    let stats = fresh_gpu(uuid)?;
    validate_power_limit(&stats, watts)?;
    privileged_smi(&[
        "-i".to_owned(),
        uuid.to_owned(),
        format!("--power-limit={watts}"),
    ])?;
    Ok(format!(
        "Power limit for GPU {} set to {watts} W",
        stats.id.index
    ))
}

pub fn set_fan_speed(uuid: &str, speed: Option<i32>) -> Result<String, String> {
    let stats = fresh_gpu(uuid)?;
    let controls = probe_controls(uuid)?;
    let fan = controls
        .fan
        .ok_or_else(|| format!("fan control blocked: {}", controls.fan_reason))?;
    if fan.targets.is_empty() {
        return Err("fan control blocked: no related fan targets".to_owned());
    }

    let control_target = gpu_target(uuid, GPU_FAN_CONTROL_STATE)?;
    match speed {
        None => {
            assign_setting(&control_target, 0)?;
            Ok(format!(
                "Automatic fan control enabled for GPU {}",
                stats.id.index
            ))
        }
        Some(value) => {
            if !fan.range.contains(value) {
                return Err(format!(
                    "fan speed {value}% is outside the current verified range {}-{}%",
                    fan.range.min, fan.range.max
                ));
            }
            assign_setting(&control_target, 1)?;
            let target = fan_qualifier_target(uuid, GPU_TARGET_FAN_SPEED)?;
            assign_setting(&target, value)?;
            Ok(format!(
                "{} related fan(s) on GPU {} set to {value}%",
                fan.targets.len(),
                stats.id.index
            ))
        }
    }
}

pub fn set_tuning(
    uuid: &str,
    core_offset: Option<i32>,
    memory_offset: Option<i32>,
) -> Result<String, String> {
    if core_offset.is_none() && memory_offset.is_none() {
        return Err("no supported tuning value was requested".to_owned());
    }
    let stats = fresh_gpu(uuid)?;
    let controls = probe_controls(uuid)?;

    if let Some(value) = core_offset {
        let range = controls
            .core_offset
            .as_ref()
            .ok_or_else(|| format!("core tuning blocked: {}", controls.core_reason))?;
        if !range.contains(value) {
            return Err(format!(
                "core offset {value} is outside the current verified range {} to {}",
                range.min, range.max
            ));
        }
    }
    if let Some(value) = memory_offset {
        let range = controls
            .memory_offset
            .as_ref()
            .ok_or_else(|| format!("memory tuning blocked: {}", controls.memory_reason))?;
        if !range.contains(value) {
            return Err(format!(
                "memory offset {value} is outside the current verified range {} to {}",
                range.min, range.max
            ));
        }
    }

    if let Some(value) = core_offset {
        assign_setting(&gpu_target(uuid, CORE_OFFSET)?, value)?;
    }
    if let Some(value) = memory_offset {
        assign_setting(&gpu_target(uuid, MEMORY_OFFSET)?, value)?;
    }
    Ok(format!("Tuning applied to GPU {}", stats.id.index))
}

fn fresh_gpu(uuid: &str) -> Result<GpuStats, String> {
    validate_uuid(uuid)?;
    let snapshot = read_inventory()?;
    let mut matches = snapshot.gpus.into_iter().filter(|gpu| gpu.id.uuid == uuid);
    let gpu = matches
        .next()
        .ok_or_else(|| "selected GPU is no longer present; change blocked".to_owned())?;
    if matches.next().is_some() {
        return Err("duplicate GPU UUID reported; change blocked".to_owned());
    }
    Ok(gpu)
}

fn validate_power_limit(stats: &GpuStats, watts: u32) -> Result<(), String> {
    let (min, max) = stats
        .power_range()
        .ok_or_else(|| "driver did not report a safe power-limit range".to_owned())?;
    if !(min..=max).contains(&watts) {
        return Err(format!(
            "power limit {watts} W is outside the current driver range {min}-{max} W"
        ));
    }
    Ok(())
}

fn privileged_smi(args: &[String]) -> Result<(), String> {
    let pkexec = resolve_executable("pkexec", &["/run/wrappers/bin/pkexec"])?;
    let nvidia_smi = resolve_executable(
        "nvidia-smi",
        &[
            "/run/current-system/sw/bin/nvidia-smi",
            "/run/opengl-driver/bin/nvidia-smi",
        ],
    )?;
    let output = Command::new(pkexec)
        .arg(nvidia_smi)
        .args(args)
        .output()
        .map_err(|error| format!("could not start {pkexec}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(command_failure("privileged nvidia-smi", &output))
}

fn verify_settings_gpu(uuid: &str) -> Result<(), String> {
    let target = gpu_target(uuid, "GpuUUID")?;
    let output = run_nvidia_settings(&["-q".to_owned(), target, "-t".to_owned()])?;
    let identities: Vec<_> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if identities.len() != 1 || identities[0] != uuid {
        return Err("nvidia-settings GPU identity did not match the selected UUID".to_owned());
    }
    Ok(())
}

fn optional_gpu_range(target: &str, label: &str) -> (Option<SettingRange>, String) {
    match query_setting_instances(target, "gpu") {
        Ok(instances) if instances.len() == 1 => (Some(instances[0].range.clone()), String::new()),
        Ok(_) => (
            None,
            format!("{label} are unavailable because the GPU target was ambiguous."),
        ),
        Err(error) => (None, format!("{label} are unavailable: {error}")),
    }
}

fn build_fan_control(manual: bool, instances: Vec<SettingInstance>) -> Result<FanControl, String> {
    if instances.is_empty() {
        return Err("no related fan targets".to_owned());
    }
    let mut targets = Vec::with_capacity(instances.len());
    let mut seen = HashSet::new();
    let mut min = i32::MIN;
    let mut max = i32::MAX;
    let mut current = None;
    for instance in instances {
        if !seen.insert(instance.target_id) {
            return Err("duplicate fan target".to_owned());
        }
        targets.push(instance.target_id);
        min = min.max(instance.range.min);
        max = max.min(instance.range.max);
        current.get_or_insert(instance.range.current);
    }
    if min > max {
        return Err("related fans have no common safe range".to_owned());
    }
    targets.sort_unstable();
    Ok(FanControl {
        range: SettingRange {
            current: current.unwrap_or(min).clamp(min, max),
            min,
            max,
        },
        manual,
        targets,
    })
}

fn query_setting_value(target: &str) -> Result<i32, String> {
    let output = run_nvidia_settings(&["-q".to_owned(), target.to_owned()])?;
    parse_setting_value(&output).ok_or_else(|| "could not parse driver value".to_owned())
}

fn query_setting_instances(
    target: &str,
    target_kind: &str,
) -> Result<Vec<SettingInstance>, String> {
    let output = run_nvidia_settings(&["-q".to_owned(), target.to_owned()])?;
    parse_setting_instances(&output, target_kind)
}

fn run_nvidia_settings(args: &[String]) -> Result<String, String> {
    let binary = resolve_executable(
        "nvidia-settings",
        &["/run/current-system/sw/bin/nvidia-settings"],
    )?;
    let output = Command::new(binary)
        .args(args)
        .output()
        .map_err(|error| format!("could not start {binary}: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(command_failure("nvidia-settings", &output))
    }
}

fn assign_setting(target: &str, value: i32) -> Result<(), String> {
    run_nvidia_settings(&["-a".to_owned(), format!("{target}={value}")]).map(|_| ())
}

fn gpu_target(uuid: &str, attribute: &str) -> Result<String, String> {
    validate_uuid(uuid)?;
    Ok(format!("[GPU:{uuid}]/{attribute}"))
}

fn fan_qualifier_target(uuid: &str, attribute: &str) -> Result<String, String> {
    validate_uuid(uuid)?;
    Ok(format!("[GPU:{uuid}.FAN]/{attribute}"))
}

fn validate_uuid(uuid: &str) -> Result<(), String> {
    valid_gpu_uuid(uuid)
        .then_some(())
        .ok_or_else(|| "invalid GPU UUID; command blocked".to_owned())
}

fn resolve_executable<'a>(name: &str, paths: &'a [&str]) -> Result<&'a str, String> {
    paths
        .iter()
        .copied()
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| {
            format!(
                "{name} was not found at a trusted system path ({})",
                paths.join(", ")
            )
        })
}

fn command_failure(name: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    format!("{name} failed: {detail}")
}

fn parse_setting_value(output: &str) -> Option<i32> {
    output
        .lines()
        .find(|line| line.trim_start().starts_with("Attribute '"))
        .and_then(parse_attribute_value)
}

fn parse_attribute_value(line: &str) -> Option<i32> {
    line.rsplit_once(": ")
        .and_then(|(_, value)| value.trim().trim_end_matches('.').parse().ok())
}

fn parse_setting_range(output: &str) -> Option<(i32, i32)> {
    let range = output
        .lines()
        .find_map(|line| line.split_once("range ").map(|(_, range)| range))?
        .split_once(" (inclusive)")?
        .0;
    let (min, max) = range.split_once(" - ")?;
    Some((min.trim().parse().ok()?, max.trim().parse().ok()?))
}

fn parse_setting_instances(
    output: &str,
    target_kind: &str,
) -> Result<Vec<SettingInstance>, String> {
    let marker = format!("([{target_kind}:");
    let mut instances = Vec::new();
    for block in output.split("Attribute '").skip(1) {
        let header = block
            .lines()
            .next()
            .ok_or_else(|| "empty attribute block".to_owned())?;
        let (_, target_and_value) = header
            .split_once(&marker)
            .ok_or_else(|| format!("unexpected target type in '{header}'"))?;
        let (target_id, _) = target_and_value
            .split_once("])")
            .ok_or_else(|| format!("malformed target in '{header}'"))?;
        let target_id = target_id
            .parse::<u32>()
            .map_err(|_| format!("invalid target identifier in '{header}'"))?;
        let current = parse_attribute_value(header)
            .ok_or_else(|| format!("invalid setting value in '{header}'"))?;
        let (min, max) = parse_setting_range(block)
            .ok_or_else(|| format!("missing setting range for target {target_id}"))?;
        if min > max || !((min as i64)..=(max as i64)).contains(&(current as i64)) {
            return Err(format!("inconsistent range for target {target_id}"));
        }
        instances.push(SettingInstance {
            target_id,
            range: SettingRange { current, min, max },
        });
    }
    if instances.is_empty() {
        return Err("driver returned no matching setting targets".to_owned());
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::InventorySnapshot;

    const UUID: &str = "GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    const GPU_LINE: &str = "0, GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee, 00000000:01:00.0, NVIDIA RTX, 55, 71, 12, 1024, 8192, 44, 120.5, 250, 100, 300, 250, 1800, 7000, 2100, 8000, 4, 16, 615.1, 95.02";

    #[test]
    fn constructs_uuid_scoped_targets() {
        assert_eq!(
            gpu_target(UUID, CORE_OFFSET).as_deref(),
            Ok("[GPU:GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee]/GPUGraphicsClockOffsetAllPerformanceLevels")
        );
        assert_eq!(
            fan_qualifier_target(UUID, GPU_TARGET_FAN_SPEED).as_deref(),
            Ok("[GPU:GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.FAN]/GPUTargetFanSpeed")
        );
    }

    #[test]
    fn rejects_injection_in_target_identifier() {
        assert!(gpu_target("GPU-0]/x=1", CORE_OFFSET).is_err());
        assert!(fan_qualifier_target("$(touch /tmp/x)", GPU_TARGET_FAN_SPEED).is_err());
    }

    #[test]
    fn parses_multiple_related_fans() {
        let output = "  Attribute 'GPUTargetFanSpeed' ([fan:0]): 30.\n    The valid values for 'GPUTargetFanSpeed' are in the range 30 - 100 (inclusive).\n  Attribute 'GPUTargetFanSpeed' ([fan:1]): 35.\n    The valid values for 'GPUTargetFanSpeed' are in the range 35 - 90 (inclusive).\n";
        let instances = parse_setting_instances(output, "fan").expect("valid fan output");
        let control = build_fan_control(false, instances).expect("compatible fan ranges");
        assert_eq!(control.targets, vec![0, 1]);
        assert_eq!(control.range.min, 35);
        assert_eq!(control.range.max, 90);
    }

    #[test]
    fn rejects_duplicate_or_incompatible_fan_mapping() {
        let duplicate = vec![
            SettingInstance {
                target_id: 0,
                range: SettingRange {
                    current: 30,
                    min: 30,
                    max: 100,
                },
            },
            SettingInstance {
                target_id: 0,
                range: SettingRange {
                    current: 30,
                    min: 30,
                    max: 100,
                },
            },
        ];
        assert!(build_fan_control(false, duplicate).is_err());

        let incompatible = vec![
            SettingInstance {
                target_id: 0,
                range: SettingRange {
                    current: 20,
                    min: 10,
                    max: 30,
                },
            },
            SettingInstance {
                target_id: 1,
                range: SettingRange {
                    current: 50,
                    min: 40,
                    max: 60,
                },
            },
        ];
        assert!(build_fan_control(false, incompatible).is_err());
    }

    #[test]
    fn validates_power_limit_boundaries() {
        let snapshot = InventorySnapshot::parse_csv(GPU_LINE);
        let stats = &snapshot.gpus[0];
        assert!(validate_power_limit(stats, 99).is_err());
        assert!(validate_power_limit(stats, 100).is_ok());
        assert!(validate_power_limit(stats, 250).is_ok());
        assert!(validate_power_limit(stats, 300).is_ok());
        assert!(validate_power_limit(stats, 301).is_err());
    }

    #[test]
    fn parses_negative_setting_range() {
        let output = "  Attribute 'GPUGraphicsClockOffsetAllPerformanceLevels' ([gpu:0]): 0.\n    The valid values for 'GPUGraphicsClockOffsetAllPerformanceLevels' are in the range -1000 - 1000 (inclusive).\n";
        let instances = parse_setting_instances(output, "gpu").expect("valid range");
        assert_eq!(instances[0].range.min, -1000);
        assert_eq!(instances[0].range.max, 1000);
    }

    #[test]
    fn rejects_malformed_setting_output() {
        assert!(parse_setting_instances("Attribute 'x' ([fan:bad]): N/A.", "fan").is_err());
        assert!(parse_setting_instances("", "fan").is_err());
    }
}
