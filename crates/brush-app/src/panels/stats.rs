use crate::app::{AppContext, AppPanel};
use brush_process::process_loop::ProcessMessage;
use burn_jit::cubecl::Runtime;
use burn_wgpu::{WgpuDevice, WgpuRuntime};
use rust_i18n::t;
use std::time::Duration;
use web_time::Instant;
use wgpu::AdapterInfo;
pub(crate) struct StatsPanel {
    device: WgpuDevice,

    last_train_step: (Instant, u32),
    train_iter_per_s: f32,
    last_eval: Option<String>,
    cur_sh_degree: u32,

    training_started: bool,
    num_splats: usize,
    frames: usize,

    start_load_time: Instant,
    adapter_info: AdapterInfo,
}

impl StatsPanel {
    pub(crate) fn new(device: WgpuDevice, adapter_info: AdapterInfo) -> Self {
        Self {
            device,
            last_train_step: (Instant::now(), 0),
            train_iter_per_s: 0.0,
            last_eval: None,
            training_started: false,
            num_splats: 0,
            frames: 0,
            cur_sh_degree: 0,
            start_load_time: Instant::now(),
            adapter_info,
        }
    }
}

fn bytes_format(bytes: u64) -> String {
    let unit = 1000;

    if bytes < unit {
        format!("{bytes} B")
    } else {
        let size = bytes as f64;
        let exp = match size.log(1000.0).floor() as usize {
            0 => 1,
            e => e,
        };
        let unit_prefix = b"KMGTPEZY";
        format!(
            "{:.2} {}B",
            (size / unit.pow(exp as u32) as f64),
            unit_prefix[exp - 1] as char,
        )
    }
}

impl AppPanel for StatsPanel {
    fn title(&self) -> String {
        t!("stat").into_owned()
    }

    fn on_message(&mut self, message: &ProcessMessage, _: &mut AppContext) {
        match message {
            ProcessMessage::NewSource => {
                *self = Self::new(self.device.clone(), self.adapter_info.clone());
            }
            ProcessMessage::StartLoading { training } => {
                self.start_load_time = Instant::now();
                self.last_train_step = (Instant::now(), 0);
                self.train_iter_per_s = 0.0;
                self.num_splats = 0;
                self.cur_sh_degree = 0;
                self.last_eval = None;
                self.training_started = *training;
            }
            ProcessMessage::ViewSplats {
                up_axis: _,
                splats,
                frame,
                total_frames: _,
            } => {
                self.num_splats = splats.num_splats();
                self.frames = *frame;
                self.cur_sh_degree = splats.sh_degree();
            }
            ProcessMessage::TrainStep {
                splats,
                stats: _,
                iter,
                timestamp,
            } => {
                self.cur_sh_degree = splats.sh_degree();
                self.num_splats = splats.num_splats();
                let current_iter_per_s = (iter - self.last_train_step.1) as f32
                    / (*timestamp - self.last_train_step.0).as_secs_f32();
                self.train_iter_per_s = 0.95 * self.train_iter_per_s + 0.05 * current_iter_per_s;
                self.last_train_step = (*timestamp, *iter);
            }
            ProcessMessage::EvalResult {
                iter: _,
                avg_psnr,
                avg_ssim,
            } => {
                self.last_eval = Some(format!("{avg_psnr:.2} PSNR, {avg_ssim:.3} SSIM"));
            }
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut AppContext) {
        egui::Grid::new("stats_grid")
            .num_columns(2)
            .spacing([40.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(t!("splat-points"));
                ui.label(format!("{}", self.num_splats));
                ui.end_row();

                ui.label("SH Degree");
                ui.label(format!("{}", self.cur_sh_degree));
                ui.end_row();

                if self.frames > 0 {
                    ui.label("Frames");
                    ui.label(format!("{}", self.frames));
                    ui.end_row();
                }

                if self.training_started {
                    ui.label(t!("train-steps"));
                    ui.label(format!("{}", self.last_train_step.1));
                    ui.end_row();

                    ui.label(t!("steps-per-second"));
                    ui.label(format!("{:.1}", self.train_iter_per_s));
                    ui.end_row();

                    ui.label(t!("last-eval"));
                    ui.label(if let Some(eval) = self.last_eval.as_ref() {
                        eval
                    } else {
                        "--"
                    });
                    ui.end_row();

                    ui.label(t!("training-time"));
                    // Round duration to seconds.
                    let elapsed = Duration::from_secs(self.start_load_time.elapsed().as_secs());
                    ui.label(format!("{}", humantime::Duration::from(elapsed)));
                    ui.end_row();
                }

                let client = WgpuRuntime::client(&self.device);
                let memory = client.memory_usage();

                ui.label(t!("gpu-memory"));
                ui.end_row();

                ui.label(t!("bytes-in-use"));
                ui.label(bytes_format(memory.bytes_in_use));
                ui.end_row();

                ui.label(t!("bytes-reserved"));
                ui.label(bytes_format(memory.bytes_reserved));
                ui.end_row();

                ui.label(t!("active-allocations"));
                ui.label(format!("{}", memory.number_allocs));
                ui.end_row();
            });

        // On WASM, adapter info is mostly private, not worth showing.
        if !cfg!(target_family = "wasm") {
            egui::Grid::new("gpu_grid")
                .num_columns(2)
                .spacing([40.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(t!("gpu-info"));
                    ui.end_row();

                    ui.label(t!("gpu-name"));
                    ui.label(&self.adapter_info.name);
                    ui.end_row();

                    ui.label(t!("gpu-type"));
                    ui.label(format!("{:?}", self.adapter_info.device_type));
                    ui.end_row();

                    ui.label(t!("gpu-driver"));
                    ui.label(format!(
                        "{}, {}",
                        self.adapter_info.driver, self.adapter_info.driver_info
                    ));
                    ui.end_row();
                });
        }
    }
}
