use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::{
    gpu::InventorySnapshot,
    nvidia::{self, GpuControls},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Power,
    Fan,
    Tuning,
}

pub enum WorkerRequest {
    Poll,
    ProbeControls {
        uuid: String,
    },
    SetPower {
        uuid: String,
        watts: u32,
    },
    SetFan {
        uuid: String,
        speed: Option<i32>,
    },
    SetTuning {
        uuid: String,
        core_offset: Option<i32>,
        memory_offset: Option<i32>,
    },
}

pub enum WorkerResponse {
    Poll(Result<InventorySnapshot, String>),
    Controls {
        uuid: String,
        result: Result<GpuControls, String>,
    },
    Action {
        uuid: String,
        kind: ActionKind,
        result: Result<String, String>,
    },
}

pub struct Worker {
    pub requests: Sender<WorkerRequest>,
    pub responses: Receiver<WorkerResponse>,
}

impl Worker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let _handle = thread::Builder::new()
            .name("nvidia-gpu-worker".to_owned())
            .spawn(move || run_worker(request_rx, response_tx));
        Self {
            requests: request_tx,
            responses: response_rx,
        }
    }
}

fn run_worker(requests: Receiver<WorkerRequest>, responses: Sender<WorkerResponse>) {
    while let Ok(request) = requests.recv() {
        let response = match request {
            WorkerRequest::Poll => WorkerResponse::Poll(nvidia::read_inventory()),
            WorkerRequest::ProbeControls { uuid } => {
                let result = nvidia::probe_controls(&uuid);
                WorkerResponse::Controls { uuid, result }
            }
            WorkerRequest::SetPower { uuid, watts } => {
                let result = nvidia::set_power_limit(&uuid, watts);
                WorkerResponse::Action {
                    uuid,
                    kind: ActionKind::Power,
                    result,
                }
            }
            WorkerRequest::SetFan { uuid, speed } => {
                let result = nvidia::set_fan_speed(&uuid, speed);
                WorkerResponse::Action {
                    uuid,
                    kind: ActionKind::Fan,
                    result,
                }
            }
            WorkerRequest::SetTuning {
                uuid,
                core_offset,
                memory_offset,
            } => {
                let result = nvidia::set_tuning(&uuid, core_offset, memory_offset);
                WorkerResponse::Action {
                    uuid,
                    kind: ActionKind::Tuning,
                    result,
                }
            }
        };
        if responses.send(response).is_err() {
            break;
        }
    }
}
