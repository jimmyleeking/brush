use egui::epaint::mutex::RwLock as EguiRwLock;
use std::sync::Arc;

use async_channel::Receiver;
use brush_render::gaussian_splats::Splats;
use eframe::egui_wgpu::Renderer;
use egui::{Color32, Rect};
use glam::Vec2;
use tracing::trace_span;
use web_time::Instant;
use wgpu::CommandEncoderDescriptor;

use crate::{
    burn_texture::BurnTexture,
    viewer::{ViewerContext, ViewerMessage},
    ViewerPane,
};

pub(crate) struct ScenePanel {
    pub(crate) backbuffer: Option<BurnTexture>,
    pub(crate) last_draw: Instant,
    pub(crate) last_message: Option<ViewerMessage>,

    receiver: Receiver<ViewerMessage>,

    queue: Arc<wgpu::Queue>,
    device: Arc<wgpu::Device>,
    renderer: Arc<EguiRwLock<Renderer>>,

    last_train_step: (Instant, u32),
    train_iter_per_s: f32,
}

impl ScenePanel {
    pub(crate) fn new(
        queue: Arc<wgpu::Queue>,
        device: Arc<wgpu::Device>,
        renderer: Arc<EguiRwLock<Renderer>>,
        receiver: Receiver<ViewerMessage>,
    ) -> Self {
        Self {
            backbuffer: None,
            last_draw: Instant::now(),
            last_message: None,
            receiver,
            queue,
            device,
            renderer,
            last_train_step: (Instant::now(), 0),
            train_iter_per_s: 0.0,
        }
    }

    pub(crate) fn draw_splats(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut ViewerContext,
        splats: &Splats<brush_render::PrimaryBackend>,
        background: glam::Vec3,
    ) {
        let size = ui.available_size();

        // Round to 64 pixels for buffer alignment.
        let size = glam::uvec2(((size.x as u32 / 64) * 64).max(32), (size.y as u32).max(32));

        let (rect, response) = ui.allocate_exact_size(
            egui::Vec2::new(size.x as f32, size.y as f32),
            egui::Sense::drag(),
        );

        let mouse_delta = glam::vec2(response.drag_delta().x, response.drag_delta().y);

        let (pan, rotate) = if response.dragged_by(egui::PointerButton::Primary) {
            (Vec2::ZERO, mouse_delta)
        } else if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            (mouse_delta, Vec2::ZERO)
        } else {
            (Vec2::ZERO, Vec2::ZERO)
        };

        let scrolled = ui.input(|r| r.smooth_scroll_delta).y;
        let cur_time = Instant::now();
        let delta_time = cur_time - self.last_draw;
        self.last_draw = cur_time;

        context.controls.pan_orbit_camera(
            &mut context.camera,
            pan * 5.0,
            rotate * 5.0,
            scrolled * 0.01,
            glam::vec2(rect.size().x, rect.size().y),
            delta_time.as_secs_f32(),
        );

        // let base_fov = 0.75;
        // self.camera.fov =
        //     glam::vec2(base_fov, base_fov * (size.y as f32) / (size.x as f32));

        // If there's actual rendering to do, not just an imgui update.
        if ui.ctx().has_requested_repaint() {
            let _span = trace_span!("Render splats").entered();
            let (img, _) = splats.render(&context.camera, size, background, true);

            let back = self.backbuffer.get_or_insert_with(|| {
                BurnTexture::new(img.clone(), &self.device, self.renderer.clone())
            });

            let mut encoder = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("viewer encoder"),
                });
            back.update_texture(img, &self.device, self.renderer.clone(), &mut encoder);
            self.queue.submit([encoder.finish()]);
        }

        if let Some(back) = self.backbuffer.as_ref() {
            ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
            ui.painter().image(
                back.id,
                rect,
                Rect {
                    min: egui::pos2(0.0, 0.0),
                    max: egui::pos2(1.0, 1.0),
                },
                Color32::WHITE,
            );
        }
    }
}

impl ViewerPane for ScenePanel {
    fn title(&self) -> String {
        "Scene".to_owned()
    }

    fn ui(&mut self, ui: &mut egui::Ui, context: &mut ViewerContext) -> egui_tiles::UiResponse {
        let _span = trace_span!("Draw UI").entered();

        ui.add_space(25.0);

        if let Ok(message) = self.receiver.try_recv() {
            self.last_message = Some(message);
        }

        let message = self.last_message.clone();

        if let Some(message) = message {
            match message {
                ViewerMessage::StartLoading => {
                    // TODO: Reset the state.
                    // self.splat_view = SplatView::new();
                    ui.label("Loading...");
                }
                ViewerMessage::Error(e) => {
                    ui.label("Error: ".to_owned() + &e.to_string());
                }
                ViewerMessage::Dataset(d) => {
                    // If this is the firs train scene, copy the initial view as a starting point.
                    if context.dataset.train.views().is_empty() && !d.train.views().is_empty() {
                        let view = &d.train.views()[0];
                        context.focus_view(&view.camera);
                    }
                    context.dataset = d;
                    ui.label("Loading dataset...");
                }
                ViewerMessage::SplatLoad {
                    splats,
                    total_count,
                } => {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} splats", splats.num_splats()));

                        if splats.num_splats() < total_count {
                            ui.label(format!(
                                "Loading... ({}%)",
                                splats.num_splats() as f32 / total_count as f32 * 100.0
                            ));
                        }
                    });

                    self.draw_splats(ui, context, &splats, glam::Vec3::ZERO);
                }
                ViewerMessage::TrainStep {
                    splats,
                    loss,
                    iter,
                    timestamp,
                } => {
                    if iter != self.last_train_step.1 {
                        self.train_iter_per_s = (iter - self.last_train_step.1) as f32
                            / (timestamp - self.last_train_step.0).as_secs_f32();
                        self.last_train_step = (timestamp, iter);
                    }

                    ui.horizontal(|ui| {
                        ui.label(format!("{} splats", splats.num_splats()));
                        ui.label(format!(
                            "Train step {iter}, {:.1} steps/s",
                            self.train_iter_per_s
                        ));

                        ui.label(format!("loss: {loss:.3e}"));

                        // let mut shared = self.train_state.shared.write();
                        // let paused = shared.paused;
                        // ui.toggle_value(&mut shared.paused, if paused { "⏵" } else { "⏸" });
                    });

                    self.draw_splats(ui, context, &splats, context.dataset.train.background);
                }
            }
        }

        if context.controls.is_animating() {
            ui.ctx().request_repaint();
        }

        egui_tiles::UiResponse::None
    }
}
