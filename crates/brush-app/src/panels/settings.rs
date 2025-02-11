
use crate::app::{AppContext, AppPanel};
use brush_dataset::{LoadDataseConfig, ModelConfig};
use brush_process::{
    data_source::DataSource,
    process_loop::{start_process, ProcessArgs, ProcessConfig, RerunConfig},
};
use brush_train::train::TrainConfig;
use egui::Slider;



use rust_i18n::t;

pub(crate) struct SettingsPanel {
    args: ProcessArgs,
    url: String,
}

impl SettingsPanel {
    pub(crate) fn new() -> Self {
        Self {
            // Nb: Important to just start with the default values here, so CLI and UI match defaults.
            args: ProcessArgs::new(
                TrainConfig::new(),
                ModelConfig::new(),
                LoadDataseConfig::new(),
                ProcessConfig::new(),
                RerunConfig::new(),
            ),
            url: "splat.com/example.ply".to_owned(),
        }
    }
}

impl AppPanel for SettingsPanel {

    fn title(&self) -> String {
        // t!("title-settings")
        t!("title-settings").into_owned()
    }

    fn ui(&mut self, ui: &mut egui::Ui, context: &mut AppContext) {

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(t!("training-data"));
            ui.label(t!("training-data-prompt"));
            let file = ui.button(t!("load-file")).clicked();
            let can_pick_dir = !cfg!(target_family = "wasm") && !cfg!(target_os = "android");
            let dir = can_pick_dir && ui.button(t!("load-dir")).clicked();
            ui.heading(t!("training-settings"));
            ui.horizontal(|ui| {
                ui.label(t!("training-steps"));

                ui.add(
                    egui::Slider::new(&mut self.args.train_config.total_steps, 1..=50000)
                        .clamping(egui::SliderClamping::Never)
                        .suffix(t!("space_steps")),
                );
            });


            #[cfg(not(target_family = "wasm"))]
            {
                ui.horizontal(|ui| {
                    ui.label(t!("export"));
                    ui.add(
                        egui::Slider::new(&mut self.args.process_config.export_every, 1..=15000)
                            .clamping(egui::SliderClamping::Never)
                            .prefix(t!("export-every"))
                            .suffix(t!("space_steps")),
                    );
                });
            }


            if file || dir  {
                let source = if file {
                    DataSource::PickFile
                } else if dir {
                    DataSource::PickDirectory
                } else {
                    DataSource::Url(self.url.clone())
                };
                context.connect_to(start_process(
                    source,
                    self.args.clone(),
                    context.device.clone(),
                ));
            }

            ui.add_space(10.0);
        });
    }
}
