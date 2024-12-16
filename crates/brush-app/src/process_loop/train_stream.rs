use async_fn_stream::try_fn_stream;

use brush_dataset::{scene_loader::SceneLoader, Dataset};
use brush_render::gaussian_splats::Splats;
use brush_train::train::{RefineStats, SplatTrainer, TrainConfig, TrainStepStats};
use burn::{backend::Autodiff, module::AutodiffModule};
use burn_wgpu::{Wgpu, WgpuDevice};
use tokio_stream::Stream;
use tracing::Instrument;
use web_time::Instant;

pub enum TrainMessage {
    TrainStep {
        splats: Box<Splats<Wgpu>>,
        stats: Box<TrainStepStats<Autodiff<Wgpu>>>,
        iter: u32,
        timestamp: Instant,
    },
    RefineStep {
        stats: Box<RefineStats>,
        iter: u32,
    },
}

// False positive: need to pass in TrainConfig by value to keep lifetimes sane.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn train_stream(
    dataset: Dataset,
    splats: Splats<Autodiff<Wgpu>>,
    config: TrainConfig,
    device: WgpuDevice,
) -> impl Stream<Item = anyhow::Result<TrainMessage>> {
    try_fn_stream(|emitter| async move {
        let mut splats = splats;

        let train_scene = dataset.train.clone();

        // TODO: Not really supported atm.
        let batch_size = 1;

        let mut dataloader = SceneLoader::new(&train_scene, batch_size, config.seed, &device);
        let mut trainer = SplatTrainer::new(&splats, &config, &device);

        let mut iter = 0;

        #[allow(clippy::infinite_loop)]
        loop {
            let batch = dataloader.next_batch().await;
            let extent = batch.scene_extent;

            let (new_splats, stats) = trainer
                .step(iter, batch, splats)
                .instrument(tracing::info_span!("Train step"))
                .await;
            let (new_splats, refine) = trainer.refine_if_needed(iter, new_splats, extent).await;
            iter += 1;
            splats = new_splats;

            emitter
                .emit(TrainMessage::TrainStep {
                    splats: Box::new(splats.valid()),
                    stats: Box::new(stats),
                    iter,
                    timestamp: Instant::now(),
                })
                .await;

            if let Some(refine) = refine {
                emitter
                    .emit(TrainMessage::RefineStep {
                        stats: Box::new(refine),
                        iter,
                    })
                    .await;
            }
        }
    })
}
