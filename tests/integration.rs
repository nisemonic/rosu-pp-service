use std::path::Path;

use rosu_pp::{any::HitResultPriority, Beatmap, Difficulty, Performance};

mod proto {
    tonic::include_proto!("pp.v1");
}

use proto::performance_service_client::PerformanceServiceClient;
use proto::{
    BeatmapInfoRequest, BeatmapSource, DifficultyRequest, DifficultySettings,
    GradualDifficultyRequest, GradualPerformanceRequest, PerformanceRequest, ScoreParams,
};

fn grpc_addr() -> String {
    std::env::var("GRPC_ADDR").unwrap_or_else(|_| "http://localhost:50051".to_string())
}

const BEATMAPS_DIR: &str = "beatmaps";
const TOLERANCE: f64 = 0.0001;

fn approx_eq(a: f64, b: f64) -> bool {
    if a == b { return true; }
    let diff = (a - b).abs();
    let max = a.abs().max(b.abs()).max(1.0);
    diff < TOLERANCE || diff / max < TOLERANCE
}

fn get_test_beatmaps() -> Vec<&'static str> {
    vec!["1000026.osu", "1000029.osu", "1000052.osu", "1000075.osu", "1000094.osu"]
}

async fn connect() -> PerformanceServiceClient<tonic::transport::Channel> {
    PerformanceServiceClient::connect(grpc_addr())
        .await
        .expect("Failed to connect to gRPC server")
}

#[tokio::test]
async fn test_difficulty_matches_rosu_pp() {
    let mut client = connect().await;

    for beatmap_name in get_test_beatmaps() {
        let path = Path::new(BEATMAPS_DIR).join(beatmap_name);
        if !path.exists() {
            eprintln!("Skipping {}: not found", beatmap_name);
            continue;
        }

        // Direct rosu-pp call
        let beatmap = Beatmap::from_path(&path).expect("parse failed");
        let expected = Difficulty::new().calculate(&beatmap);

        // gRPC call
        let content = std::fs::read(&path).expect("read failed");
        let response = client
            .calculate_difficulty(DifficultyRequest {
                beatmap: Some(BeatmapSource {
                    source: Some(proto::beatmap_source::Source::Content(content)),
                }),
                mode: None,
                settings: None,
            })
            .await
            .expect("gRPC failed")
            .into_inner();

        assert!(
            approx_eq(expected.stars(), response.stars),
            "{}: stars mismatch: rosu-pp={}, grpc={}",
            beatmap_name, expected.stars(), response.stars
        );
        assert_eq!(
            expected.max_combo(), response.max_combo,
            "{}: max_combo mismatch", beatmap_name
        );

        println!("{}: stars={:.4} ✓", beatmap_name, response.stars);
    }
}

#[tokio::test]
async fn test_performance_matches_rosu_pp() {
    let mut client = connect().await;

    for beatmap_name in get_test_beatmaps() {
        let path = Path::new(BEATMAPS_DIR).join(beatmap_name);
        if !path.exists() {
            eprintln!("Skipping {}: not found", beatmap_name);
            continue;
        }

        // Direct rosu-pp call
        let beatmap = Beatmap::from_path(&path).expect("parse failed");
        let expected = Performance::new(&beatmap).calculate();

        // gRPC call
        let content = std::fs::read(&path).expect("read failed");
        let response = client
            .calculate_performance(PerformanceRequest {
                beatmap: Some(BeatmapSource {
                    source: Some(proto::beatmap_source::Source::Content(content)),
                }),
                mode: None,
                difficulty_settings: None,
                score: None,
            })
            .await
            .expect("gRPC failed")
            .into_inner();

        assert!(
            approx_eq(expected.pp(), response.pp),
            "{}: pp mismatch: rosu-pp={}, grpc={}",
            beatmap_name, expected.pp(), response.pp
        );
        assert!(
            approx_eq(expected.stars(), response.stars),
            "{}: stars mismatch", beatmap_name
        );

        println!("{}: pp={:.4} ✓", beatmap_name, response.pp);
    }
}

#[tokio::test]
async fn test_performance_with_accuracy_matches() {
    let mut client = connect().await;
    let beatmap_name = "1000026.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    for accuracy in [100.0, 98.0, 95.0, 90.0] {
        // Direct rosu-pp
        let beatmap = Beatmap::from_path(&path).expect("parse failed");
        let expected = Performance::new(&beatmap).accuracy(accuracy).calculate();

        // gRPC
        let content = std::fs::read(&path).expect("read failed");
        let response = client
            .calculate_performance(PerformanceRequest {
                beatmap: Some(BeatmapSource {
                    source: Some(proto::beatmap_source::Source::Content(content)),
                }),
                mode: None,
                difficulty_settings: None,
                score: Some(ScoreParams {
                    accuracy: Some(accuracy),
                    ..Default::default()
                }),
            })
            .await
            .expect("gRPC failed")
            .into_inner();

        assert!(
            approx_eq(expected.pp(), response.pp),
            "acc={}: pp mismatch: rosu-pp={}, grpc={}",
            accuracy, expected.pp(), response.pp
        );

        println!("{} @ {:.1}%: pp={:.4} ✓", beatmap_name, accuracy, response.pp);
    }
}

#[tokio::test]
async fn test_performance_with_mods_matches() {
    let mut client = connect().await;
    let beatmap_name = "1000026.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    // HD=8, HR=16, DT=64, HDDT=72, HDHR=24
    for (name, mods) in [("NM", 0u32), ("HD", 8), ("HR", 16), ("DT", 64), ("HDDT", 72)] {
        // Direct rosu-pp
        let beatmap = Beatmap::from_path(&path).expect("parse failed");
        let expected = Performance::new(&beatmap).mods(mods).calculate();

        // gRPC
        let content = std::fs::read(&path).expect("read failed");
        let response = client
            .calculate_performance(PerformanceRequest {
                beatmap: Some(BeatmapSource {
                    source: Some(proto::beatmap_source::Source::Content(content)),
                }),
                mode: None,
                difficulty_settings: Some(DifficultySettings {
                    mods: Some(mods),
                    ..Default::default()
                }),
                score: None,
            })
            .await
            .expect("gRPC failed")
            .into_inner();

        assert!(
            approx_eq(expected.pp(), response.pp),
            "{}: pp mismatch: rosu-pp={}, grpc={}",
            name, expected.pp(), response.pp
        );

        println!("{} +{}: pp={:.4} ✓", beatmap_name, name, response.pp);
    }
}

#[tokio::test]
async fn test_performance_with_misses_matches() {
    let mut client = connect().await;
    let beatmap_name = "1000026.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    for misses in [0u32, 1, 5, 10] {
        // Direct rosu-pp
        let beatmap = Beatmap::from_path(&path).expect("parse failed");
        let expected = Performance::new(&beatmap).misses(misses).calculate();

        // gRPC
        let content = std::fs::read(&path).expect("read failed");
        let response = client
            .calculate_performance(PerformanceRequest {
                beatmap: Some(BeatmapSource {
                    source: Some(proto::beatmap_source::Source::Content(content)),
                }),
                mode: None,
                difficulty_settings: None,
                score: Some(ScoreParams {
                    misses: Some(misses),
                    ..Default::default()
                }),
            })
            .await
            .expect("gRPC failed")
            .into_inner();

        assert!(
            approx_eq(expected.pp(), response.pp),
            "{}x miss: pp mismatch: rosu-pp={}, grpc={}",
            misses, expected.pp(), response.pp
        );

        println!("{} {}x: pp={:.4} ✓", beatmap_name, misses, response.pp);
    }
}

#[tokio::test]
async fn test_beatmap_info() {
    let mut client = connect().await;
    let beatmap_name = "1000094.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    let beatmap = Beatmap::from_path(&path).expect("parse failed");
    let content = std::fs::read(&path).expect("read failed");

    let response = client
        .get_beatmap_info(BeatmapInfoRequest {
            beatmap: Some(BeatmapSource {
                source: Some(proto::beatmap_source::Source::Content(content)),
            }),
        })
        .await
        .expect("gRPC failed")
        .into_inner();

    assert_eq!(response.n_objects, beatmap.hit_objects.len() as u32);
    assert_eq!(response.ar as f32, beatmap.ar);
    assert_eq!(response.cs as f32, beatmap.cs);
    assert_eq!(response.od as f32, beatmap.od);
    assert_eq!(response.hp as f32, beatmap.hp);

    println!(
        "{}: objects={}, ar={}, cs={}, od={}, hp={} ✓",
        beatmap_name, response.n_objects, response.ar, response.cs, response.od, response.hp
    );
}

#[tokio::test]
async fn test_lazer_vs_stable() {
    let mut client = connect().await;
    let beatmap_name = "1000094.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    let content = std::fs::read(&path).expect("read failed");

    // Lazer
    let beatmap = Beatmap::from_path(&path).expect("parse failed");
    let expected_lazer = Performance::new(&beatmap).lazer(true).accuracy(98.0).calculate();

    let response_lazer = client
        .calculate_performance(PerformanceRequest {
            beatmap: Some(BeatmapSource {
                source: Some(proto::beatmap_source::Source::Content(content.clone())),
            }),
            mode: None,
            difficulty_settings: Some(DifficultySettings {
                lazer: Some(true),
                ..Default::default()
            }),
            score: Some(ScoreParams {
                accuracy: Some(98.0),
                ..Default::default()
            }),
        })
        .await
        .expect("gRPC failed")
        .into_inner();

    assert!(
        approx_eq(expected_lazer.pp(), response_lazer.pp),
        "lazer: pp mismatch: rosu-pp={}, grpc={}",
        expected_lazer.pp(), response_lazer.pp
    );

    // Stable
    let beatmap = Beatmap::from_path(&path).expect("parse failed");
    let expected_stable = Performance::new(&beatmap).lazer(false).accuracy(98.0).calculate();

    let response_stable = client
        .calculate_performance(PerformanceRequest {
            beatmap: Some(BeatmapSource {
                source: Some(proto::beatmap_source::Source::Content(content)),
            }),
            mode: None,
            difficulty_settings: Some(DifficultySettings {
                lazer: Some(false),
                ..Default::default()
            }),
            score: Some(ScoreParams {
                accuracy: Some(98.0),
                ..Default::default()
            }),
        })
        .await
        .expect("gRPC failed")
        .into_inner();

    assert!(
        approx_eq(expected_stable.pp(), response_stable.pp),
        "stable: pp mismatch: rosu-pp={}, grpc={}",
        expected_stable.pp(), response_stable.pp
    );

    assert!(
        response_lazer.pp != response_stable.pp,
        "lazer and stable should differ"
    );

    println!(
        "{}: lazer={:.4} stable={:.4} ✓",
        beatmap_name, response_lazer.pp, response_stable.pp
    );
}

#[tokio::test]
async fn test_hitresult_priority() {
    let mut client = connect().await;
    let beatmap_name = "1000094.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    let content = std::fs::read(&path).expect("read failed");

    for (name, priority_proto, priority_rosu) in [
        ("BestCase", proto::HitResultPriority::BestCase, HitResultPriority::BestCase),
        ("WorstCase", proto::HitResultPriority::WorstCase, HitResultPriority::WorstCase),
        ("Fastest", proto::HitResultPriority::Fastest, HitResultPriority::Fastest),
    ] {
        let beatmap = Beatmap::from_path(&path).expect("parse failed");
        let expected = Performance::new(&beatmap)
            .accuracy(95.0)
            .hitresult_priority(priority_rosu)
            .calculate();

        let response = client
            .calculate_performance(PerformanceRequest {
                beatmap: Some(BeatmapSource {
                    source: Some(proto::beatmap_source::Source::Content(content.clone())),
                }),
                mode: None,
                difficulty_settings: None,
                score: Some(ScoreParams {
                    accuracy: Some(95.0),
                    hitresult_priority: Some(priority_proto.into()),
                    ..Default::default()
                }),
            })
            .await
            .expect("gRPC failed")
            .into_inner();

        assert!(
            approx_eq(expected.pp(), response.pp),
            "{}: pp mismatch: rosu-pp={}, grpc={}",
            name, expected.pp(), response.pp
        );

        println!("{} {}: pp={:.4} ✓", beatmap_name, name, response.pp);
    }
}

#[tokio::test]
async fn test_gradual_difficulty() {
    let mut client = connect().await;
    let beatmap_name = "1000094.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    let beatmap = Beatmap::from_path(&path).expect("parse failed");
    let mut gradual = Difficulty::new().gradual_difficulty(&beatmap);
    let content = std::fs::read(&path).expect("read failed");

    let mut stream = client
        .get_gradual_difficulty(GradualDifficultyRequest {
            beatmap: Some(BeatmapSource {
                source: Some(proto::beatmap_source::Source::Content(content)),
            }),
            mode: None,
            settings: None,
        })
        .await
        .expect("gRPC failed")
        .into_inner();

    let mut count = 0;
    while let Some(response) = stream.message().await.expect("stream error") {
        let expected = gradual.next().expect("gradual ended early");
        assert!(
            approx_eq(expected.stars(), response.stars),
            "object {}: stars mismatch: rosu-pp={}, grpc={}",
            count, expected.stars(), response.stars
        );
        count += 1;
    }

    assert!(gradual.next().is_none(), "grpc stream ended early");
    println!("{}: {} objects gradual difficulty ✓", beatmap_name, count);
}

#[tokio::test]
async fn test_gradual_performance() {
    use rosu_pp::any::ScoreState;

    let mut client = connect().await;
    let beatmap_name = "1000094.osu";
    let path = Path::new(BEATMAPS_DIR).join(beatmap_name);

    if !path.exists() {
        eprintln!("Skipping: beatmap not found");
        return;
    }

    let beatmap = Beatmap::from_path(&path).expect("parse failed");
    let mut gradual = Difficulty::new().gradual_performance(&beatmap);
    let mut state = ScoreState::new();
    let content = std::fs::read(&path).expect("read failed");

    let mut stream = client
        .get_gradual_performance(GradualPerformanceRequest {
            beatmap: Some(BeatmapSource {
                source: Some(proto::beatmap_source::Source::Content(content)),
            }),
            mode: None,
            difficulty_settings: None,
            score: None,
        })
        .await
        .expect("gRPC failed")
        .into_inner();

    let mut count = 0;
    while let Some(response) = stream.message().await.expect("stream error") {
        state.n300 += 1;
        state.max_combo += 1;
        let expected = gradual.next(state.clone()).expect("gradual ended early");

        assert!(
            approx_eq(expected.pp(), response.pp),
            "object {}: pp mismatch: rosu-pp={}, grpc={}",
            count, expected.pp(), response.pp
        );
        assert!(
            approx_eq(expected.stars(), response.stars),
            "object {}: stars mismatch", count
        );
        count += 1;
    }

    println!("{}: {} objects gradual performance ✓", beatmap_name, count);
}
