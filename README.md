# rosu-pp-service

High-performance gRPC service for osu! performance points (PP) and star rating (SR) calculations.

Built on [rosu-pp](https://github.com/MaxOhn/rosu-pp) v4.0.1.

## Features

- **Full PP/SR calculation** for all game modes (osu!, taiko, catch, mania)
- **Batch processing** with parallel execution
- **Streaming APIs** for gradual difficulty/performance
- **Beatmap caching** with LRU eviction
- **Lazer & stable** scoring modes
- **Custom difficulty settings** (AR, CS, OD, HP overrides)
- **Mod support** via bitflags and acronyms (including lazer mods)

## Quick Start

```bash
# Start service
make up

# Health check
curl http://localhost:50051/health

# Calculate PP (requires grpcurl)
grpcurl -plaintext -proto proto/pp.proto \
  -d '{"beatmap": {"path": "/beatmaps/123456.osu"}, "score": {"accuracy": 98.5}}' \
  localhost:50051 pp.v1.PerformanceService/CalculatePerformance
```

## API

| Method | Description |
|--------|-------------|
| `CalculateDifficulty` | Star rating calculation |
| `CalculatePerformance` | PP calculation |
| `CalculateBatch` | Batch PP (parallel) |
| `GetStrains` | Strain values for graphs |
| `GetBeatmapInfo` | Beatmap metadata |
| `GetGradualDifficulty` | Stream SR per object |
| `GetGradualPerformance` | Stream PP per object |

Full documentation: `http://localhost:50051/docs`

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `PP_SERVICE_ADDR` | `[::]:50051` | Listen address |
| `PP_SERVICE_CACHE_SIZE` | `1000` | Beatmap cache capacity |
| `PP_SERVICE_LOG_FORMAT` | `text` | `text` or `json` |
| `RUST_LOG` | `info` | Log level |

## Docker

```bash
make up      # Start
make down    # Stop
make logs    # View logs
make test    # Run tests
make clean   # Remove images
```

## Proto

See [proto/pp.proto](proto/pp.proto) for full schema.

## License

MIT
