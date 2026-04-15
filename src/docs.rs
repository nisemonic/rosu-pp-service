pub const DOCUMENTATION: &str = r#"# rosu-pp-service

High-performance gRPC service for osu! PP/SR calculations powered by [rosu-pp](https://github.com/MaxOhn/rosu-pp).

## Quick Start

```bash
# Start the service
make up

# Check health
curl http://localhost:50051/health

# Calculate PP
grpcurl -plaintext -proto proto/pp.proto \
  -d '{"beatmap": {"path": "/beatmaps/123456.osu"}, "score": {"accuracy": 98.5}}' \
  localhost:50051 pp.v1.PerformanceService/CalculatePerformance
```

---

## gRPC API

**Service:** `pp.v1.PerformanceService`
**Port:** `50051`

### RPC Methods

| Method | Description |
|--------|-------------|
| `CalculateDifficulty` | Calculate star rating |
| `CalculatePerformance` | Calculate PP |
| `CalculateBatch` | Batch PP calculation |
| `GetStrains` | Get strain values for graphs |
| `GetBeatmapInfo` | Get beatmap metadata |
| `GetGradualDifficulty` | Stream SR per hitobject |
| `GetGradualPerformance` | Stream PP per hitobject |

---

## Beatmap Source

Beatmaps can be provided in two ways:

```protobuf
message BeatmapSource {
  oneof source {
    bytes content = 1;  // Raw .osu file content
    string path = 2;    // Path on server (e.g., "/beatmaps/123456.osu")
  }
}
```

**Example with content:**
```bash
grpcurl -plaintext -proto proto/pp.proto \
  -d "{\"beatmap\": {\"content\": \"$(base64 -w0 map.osu)\"}}" \
  localhost:50051 pp.v1.PerformanceService/CalculateDifficulty
```

**Example with path:**
```bash
grpcurl -plaintext -proto proto/pp.proto \
  -d '{"beatmap": {"path": "/beatmaps/1000094.osu"}}' \
  localhost:50051 pp.v1.PerformanceService/CalculateDifficulty
```

---

## CalculateDifficulty

Calculate star rating and difficulty attributes.

**Request:**
```protobuf
message DifficultyRequest {
  BeatmapSource beatmap = 1;
  optional GameMode mode = 2;      // Convert to different mode
  DifficultySettings settings = 3;
}
```

**Example:**
```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "beatmap": {"path": "/beatmaps/1000094.osu"},
  "settings": {"mods": 72}
}' localhost:50051 pp.v1.PerformanceService/CalculateDifficulty
```

**Response includes:**
- `stars` - Star rating
- `max_combo` - Maximum combo
- Mode-specific attributes (aim, speed, flashlight, etc.)

---

## CalculatePerformance

Calculate performance points.

**Request:**
```protobuf
message PerformanceRequest {
  BeatmapSource beatmap = 1;
  optional GameMode mode = 2;
  DifficultySettings difficulty_settings = 3;
  ScoreParams score = 4;
}
```

**Example:**
```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "beatmap": {"path": "/beatmaps/1000094.osu"},
  "difficulty_settings": {"mods": 8, "lazer": true},
  "score": {"accuracy": 98.5, "combo": 500, "misses": 2}
}' localhost:50051 pp.v1.PerformanceService/CalculatePerformance
```

---

## DifficultySettings

```protobuf
message DifficultySettings {
  optional uint32 mods = 1;          // Mod bitflags
  optional double clock_rate = 2;    // Custom speed (overrides DT/HT)
  optional float ar = 3;             // Custom AR
  optional bool ar_with_mods = 4;    // AR is post-mod value
  optional float cs = 5;             // Custom CS
  optional bool cs_with_mods = 6;
  optional float hp = 7;             // Custom HP
  optional bool hp_with_mods = 8;
  optional float od = 9;             // Custom OD
  optional bool od_with_mods = 10;
  optional uint32 passed_objects = 11;  // Calculate up to N objects
  optional bool hardrock_offsets = 12;  // Use HR note offsets
  optional bool lazer = 13;             // Lazer scoring (default: true)
}
```

---

## ScoreParams

```protobuf
message ScoreParams {
  optional uint32 combo = 1;
  optional double accuracy = 2;      // 0-100
  optional uint32 misses = 3;
  optional uint32 n300 = 4;
  optional uint32 n100 = 5;
  optional uint32 n50 = 6;
  optional uint32 n_geki = 7;        // Mania: MAX
  optional uint32 n_katu = 8;        // Mania: 200
  optional uint32 large_tick_hits = 9;   // Lazer slider ticks
  optional uint32 small_tick_hits = 10;
  optional uint32 slider_end_hits = 11;
  optional HitResultPriority hitresult_priority = 12;
}
```

---

## Mods

Mods are specified as bitflags:

| Mod | Value | Mod | Value |
|-----|-------|-----|-------|
| NF | 1 | HD | 8 |
| EZ | 2 | HR | 16 |
| TD | 4 | DT | 64 |
| SD | 32 | NC | 576 |
| HT | 256 | FL | 1024 |
| SO | 4096 | PF | 16416 |

**Common combinations:**
- HDHR = 8 + 16 = `24`
- HDDT = 8 + 64 = `72`
- HDDTHR = 8 + 64 + 16 = `88`

---

## HitResultPriority

When only `accuracy` is provided (no specific hitresults), this controls how hitresults are generated:

```protobuf
enum HitResultPriority {
  HIT_RESULT_PRIORITY_BEST_CASE = 0;   // Maximize 300s (default)
  HIT_RESULT_PRIORITY_WORST_CASE = 1;  // Maximize 50s/100s
  HIT_RESULT_PRIORITY_FASTEST = 2;     // Fast generation (recommended)
}
```

**Recommendation:** Use `FASTEST` when calculating with accuracy only.

---

## Lazer vs Stable

The `lazer` setting affects PP calculation for osu! and mania:

```bash
# Lazer scoring (default)
grpcurl ... -d '{"difficulty_settings": {"lazer": true}, ...}'

# Stable/Classic scoring
grpcurl ... -d '{"difficulty_settings": {"lazer": false}, ...}'
```

**Differences:**
- Lazer: Slider accuracy affects PP
- Stable: Only circle accuracy matters

---

## GameMode

```protobuf
enum GameMode {
  GAME_MODE_UNSPECIFIED = 0;  // Use beatmap's native mode
  GAME_MODE_OSU = 1;          // osu!standard
  GAME_MODE_TAIKO = 2;        // osu!taiko
  GAME_MODE_CATCH = 3;        // osu!catch
  GAME_MODE_MANIA = 4;        // osu!mania
}
```

Convert an osu!standard map to taiko:
```bash
grpcurl ... -d '{"beatmap": {...}, "mode": "GAME_MODE_TAIKO"}'
```

---

## GetStrains

Get strain values for difficulty graphs.

```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "beatmap": {"path": "/beatmaps/1000094.osu"}
}' localhost:50051 pp.v1.PerformanceService/GetStrains
```

**Response:**
```json
{
  "mode": "GAME_MODE_OSU",
  "section_length": 400,
  "osu": {
    "aim": [1.2, 1.5, 2.1, ...],
    "aim_no_sliders": [...],
    "speed": [...],
    "flashlight": [...]
  }
}
```

---

## GetBeatmapInfo

Get beatmap metadata without calculating difficulty.

```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "beatmap": {"path": "/beatmaps/1000094.osu"}
}' localhost:50051 pp.v1.PerformanceService/GetBeatmapInfo
```

**Response:**
```json
{
  "version": 14,
  "mode": "GAME_MODE_OSU",
  "ar": 9.3,
  "cs": 4,
  "od": 9,
  "hp": 6,
  "bpm": 180,
  "n_circles": 500,
  "n_sliders": 200,
  "n_spinners": 3,
  "n_objects": 703
}
```

---

## Gradual Calculation (Streaming)

### GetGradualDifficulty

Stream difficulty attributes after each hitobject. Useful for SR graphs.

```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "beatmap": {"path": "/beatmaps/1000094.osu"}
}' localhost:50051 pp.v1.PerformanceService/GetGradualDifficulty
```

### GetGradualPerformance

Stream PP after each hitobject. Useful for live PP counters.

```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "beatmap": {"path": "/beatmaps/1000094.osu"},
  "score": {"accuracy": 100}
}' localhost:50051 pp.v1.PerformanceService/GetGradualPerformance
```

---

## Batch Calculation

Calculate PP for multiple scores at once.

```bash
grpcurl -plaintext -proto proto/pp.proto -d '{
  "requests": [
    {"beatmap": {"path": "/beatmaps/1000094.osu"}, "score": {"accuracy": 100}},
    {"beatmap": {"path": "/beatmaps/1000094.osu"}, "score": {"accuracy": 98}},
    {"beatmap": {"path": "/beatmaps/1000094.osu"}, "score": {"accuracy": 95}}
  ]
}' localhost:50051 pp.v1.PerformanceService/CalculateBatch
```

---

## HTTP Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check with cache stats |
| `GET /ready` | Readiness probe |
| `GET /live` | Liveness probe |
| `GET /docs` | This documentation |

---

## Caching

Beatmaps are cached by `BeatmapID` (from .osu metadata). Cache stats available at `/health`:

```json
{
  "cache": {
    "size": 42,
    "capacity": 1000,
    "hits": 1337,
    "misses": 100
  }
}
```

Configure cache size via environment variable:
```bash
PP_SERVICE_CACHE_SIZE=5000
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PP_SERVICE_ADDR` | `[::]:50051` | Listen address |
| `PP_SERVICE_CACHE_SIZE` | `1000` | Beatmap cache size |
| `PP_SERVICE_LOG_FORMAT` | `text` | `text` or `json` |
| `RUST_LOG` | `info` | Log level |

---

## Docker

```bash
# Start service
make up

# Run tests
make test

# View logs
make logs

# Stop
make down
```

**compose.yml:**
```yaml
services:
  app:
    build: .
    ports:
      - "50051:50051"
    volumes:
      - ./beatmaps:/beatmaps:ro
    environment:
      RUST_LOG: info
```

---

## Error Codes

| gRPC Code | Description |
|-----------|-------------|
| `INVALID_ARGUMENT` | Invalid request parameters |
| `FAILED_PRECONDITION` | Beatmap parse error |
| `INTERNAL` | Server error |

---

## Proto File

Full proto definition: [proto/pp.proto](proto/pp.proto)

```protobuf
service PerformanceService {
  rpc CalculateDifficulty(DifficultyRequest) returns (DifficultyResponse);
  rpc CalculatePerformance(PerformanceRequest) returns (PerformanceResponse);
  rpc CalculateBatch(BatchRequest) returns (BatchResponse);
  rpc GetStrains(StrainsRequest) returns (StrainsResponse);
  rpc GetBeatmapInfo(BeatmapInfoRequest) returns (BeatmapInfoResponse);
  rpc GetGradualDifficulty(GradualDifficultyRequest) returns (stream GradualDifficultyResponse);
  rpc GetGradualPerformance(GradualPerformanceRequest) returns (stream GradualPerformanceResponse);
}
```
"#;
