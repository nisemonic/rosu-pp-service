use std::time::Instant;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, instrument, warn};

use crate::proto::performance_service_server::PerformanceService;
use crate::proto::{
    BatchRequest, BatchResponse, BeatmapInfoRequest, BeatmapInfoResponse, DifficultyRequest,
    DifficultyResponse, GradualDifficultyRequest, GradualDifficultyResponse,
    GradualPerformanceRequest, GradualPerformanceResponse, PerformanceRequest, PerformanceResponse,
    StrainGraphRequest, StrainGraphResponse, StrainsRequest, StrainsResponse,
};
use crate::service::Calculator;

#[derive(Debug, Default)]
pub struct PerformanceServiceImpl;

fn get_mode_name(mode: Option<i32>) -> &'static str {
    match mode.unwrap_or(0) {
        1 => "osu",
        2 => "taiko",
        3 => "catch",
        4 => "mania",
        _ => "auto",
    }
}

#[tonic::async_trait]
impl PerformanceService for PerformanceServiceImpl {
    #[instrument(skip(self, request), fields(method = "CalculateDifficulty"))]
    async fn calculate_difficulty(
        &self,
        request: Request<DifficultyRequest>,
    ) -> Result<Response<DifficultyResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let mode = get_mode_name(req.mode);
        let mods = req.settings.as_ref().and_then(|s| s.mods).unwrap_or(0);

        debug!(mode, mods, "processing difficulty request");

        let result = tokio::task::spawn_blocking(move || Calculator::difficulty(&req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let elapsed = start.elapsed();
        match &result {
            Ok(resp) => info!(mode, mods, stars = resp.stars, elapsed_ms = elapsed.as_millis(), "difficulty calculated"),
            Err(e) => warn!(mode, mods, error = %e, elapsed_ms = elapsed.as_millis(), "difficulty calculation failed"),
        }

        result.map(Response::new).map_err(Into::into)
    }

    #[instrument(skip(self, request), fields(method = "CalculatePerformance"))]
    async fn calculate_performance(
        &self,
        request: Request<PerformanceRequest>,
    ) -> Result<Response<PerformanceResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let mode = get_mode_name(req.mode);
        let mods = req.difficulty_settings.as_ref().and_then(|s| s.mods).unwrap_or(0);

        debug!(mode, mods, "processing performance request");

        let result = tokio::task::spawn_blocking(move || Calculator::performance(&req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let elapsed = start.elapsed();
        match &result {
            Ok(resp) => info!(mode, mods, pp = resp.pp, stars = resp.stars, elapsed_ms = elapsed.as_millis(), "performance calculated"),
            Err(e) => warn!(mode, mods, error = %e, elapsed_ms = elapsed.as_millis(), "performance calculation failed"),
        }

        result.map(Response::new).map_err(Into::into)
    }

    #[instrument(skip(self, request), fields(method = "CalculateBatch"))]
    async fn calculate_batch(
        &self,
        request: Request<BatchRequest>,
    ) -> Result<Response<BatchResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let batch_size = req.requests.len();

        debug!(batch_size, "processing batch request");

        let result = tokio::task::spawn_blocking(move || Calculator::batch(&req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let elapsed = start.elapsed();
        match &result {
            Ok(resp) => info!(batch_size, responses = resp.responses.len(), elapsed_ms = elapsed.as_millis(), "batch calculated"),
            Err(e) => warn!(batch_size, error = %e, elapsed_ms = elapsed.as_millis(), "batch calculation failed"),
        }

        result.map(Response::new).map_err(Into::into)
    }

    #[instrument(skip(self, request), fields(method = "GetStrains"))]
    async fn get_strains(
        &self,
        request: Request<StrainsRequest>,
    ) -> Result<Response<StrainsResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let mode = get_mode_name(req.mode);
        let mods = req.settings.as_ref().and_then(|s| s.mods).unwrap_or(0);

        debug!(mode, mods, "processing strains request");

        let result = tokio::task::spawn_blocking(move || Calculator::strains(&req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let elapsed = start.elapsed();
        match &result {
            Ok(_) => info!(mode, mods, elapsed_ms = elapsed.as_millis(), "strains calculated"),
            Err(e) => warn!(mode, mods, error = %e, elapsed_ms = elapsed.as_millis(), "strains calculation failed"),
        }

        result.map(Response::new).map_err(Into::into)
    }

    #[instrument(skip(self, request), fields(method = "GetStrainGraph"))]
    async fn get_strain_graph(
        &self,
        request: Request<StrainGraphRequest>,
    ) -> Result<Response<StrainGraphResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let mode = get_mode_name(req.mode);
        let mods = req.settings.as_ref().and_then(|s| s.mods).unwrap_or(0);
        let width = req.width;
        let height = req.height;

        debug!(mode, mods, width, height, "processing strain graph request");

        let result = tokio::task::spawn_blocking(move || Calculator::strain_graph(&req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let elapsed = start.elapsed();
        match &result {
            Ok(resp) => info!(
                mode, mods, width, height,
                png_size = resp.png_data.len(),
                elapsed_ms = elapsed.as_millis(),
                "strain graph rendered"
            ),
            Err(e) => warn!(
                mode, mods, width, height,
                error = %e,
                elapsed_ms = elapsed.as_millis(),
                "strain graph rendering failed"
            ),
        }

        result.map(Response::new).map_err(Into::into)
    }

    #[instrument(skip(self, request), fields(method = "GetBeatmapInfo"))]
    async fn get_beatmap_info(
        &self,
        request: Request<BeatmapInfoRequest>,
    ) -> Result<Response<BeatmapInfoResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();

        debug!("processing beatmap info request");

        let result = tokio::task::spawn_blocking(move || Calculator::beatmap_info(&req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let elapsed = start.elapsed();
        match &result {
            Ok(resp) => info!(mode = get_mode_name(Some(resp.mode)), objects = resp.n_objects, elapsed_ms = elapsed.as_millis(), "beatmap info retrieved"),
            Err(e) => warn!(error = %e, elapsed_ms = elapsed.as_millis(), "beatmap info failed"),
        }

        result.map(Response::new).map_err(Into::into)
    }

    type GetGradualDifficultyStream = ReceiverStream<Result<GradualDifficultyResponse, Status>>;

    #[instrument(skip(self, request), fields(method = "GetGradualDifficulty"))]
    async fn get_gradual_difficulty(
        &self,
        request: Request<GradualDifficultyRequest>,
    ) -> Result<Response<Self::GetGradualDifficultyStream>, Status> {
        let req = request.into_inner();
        let mode = get_mode_name(req.mode);
        let mods = req.settings.as_ref().and_then(|s| s.mods).unwrap_or(0);
        let (tx, rx) = mpsc::channel(128);

        info!(mode, mods, "starting gradual difficulty stream");

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            match Calculator::gradual_difficulty(&req) {
                Ok(iter) => {
                    let mut count = 0u32;
                    for resp in iter {
                        if tx.blocking_send(Ok(resp)).is_err() {
                            debug!(count, "gradual difficulty stream cancelled by client");
                            break;
                        }
                        count += 1;
                    }
                    info!(count, elapsed_ms = start.elapsed().as_millis(), "gradual difficulty stream completed");
                }
                Err(e) => {
                    error!(error = %e, "gradual difficulty stream failed");
                    let _ = tx.blocking_send(Err(e.into()));
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type GetGradualPerformanceStream = ReceiverStream<Result<GradualPerformanceResponse, Status>>;

    #[instrument(skip(self, request), fields(method = "GetGradualPerformance"))]
    async fn get_gradual_performance(
        &self,
        request: Request<GradualPerformanceRequest>,
    ) -> Result<Response<Self::GetGradualPerformanceStream>, Status> {
        let req = request.into_inner();
        let mode = get_mode_name(req.mode);
        let mods = req.difficulty_settings.as_ref().and_then(|s| s.mods).unwrap_or(0);
        let (tx, rx) = mpsc::channel(128);

        info!(mode, mods, "starting gradual performance stream");

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            match Calculator::gradual_performance(&req) {
                Ok(mut calc) => {
                    let mut count = 0u32;
                    while let Some(resp) = calc.next() {
                        if tx.blocking_send(Ok(resp)).is_err() {
                            debug!(count, "gradual performance stream cancelled by client");
                            break;
                        }
                        count += 1;
                    }
                    info!(count, elapsed_ms = start.elapsed().as_millis(), "gradual performance stream completed");
                }
                Err(e) => {
                    error!(error = %e, "gradual performance stream failed");
                    let _ = tx.blocking_send(Err(e.into()));
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
