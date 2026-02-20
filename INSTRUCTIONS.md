# L2 Arbitrage Engine v5

High-performance Rust trading engine for cross-DEX arbitrage on Layer 2 networks.

## Supported Chains

| Chain    | Chain ID | Avg Gas   |
|----------|----------|-----------|
| Base     | 8453     | 0.01 gwei |
| Arbitrum | 42161    | 0.1 gwei  |
| Polygon  | 137      | 30 gwei   |

## Architecture

```
src/
├── main.rs              # 30s tick engine, HTTP API
├── rpc_rotator.rs       # 12 free public RPC endpoints
├── arbitrage.rs         # Cross-DEX arbitrage strategies
├── scanner/
│   └── analysis.rs      # Contract bytecode analysis
└── treasury/
    └── guard.rs         # Profit calculation, reserves
```

### Tick Engine

30-second tick cycle:
1. Rotate RPC endpoints and check health
2. Scan cross-DEX price discrepancies
3. Evaluate opportunities against P_net threshold
4. Size positions using Kelly criterion
5. Execute profitable trades

### Engine Modes

- **Bootstrap** — Conservative: 80% arbitrage + 20% scanning
- **Standard** — Full operation: 50% arbitrage + 20% scanning + 30% advanced

### Risk Management

| Parameter       | Value |
|-----------------|-------|
| Kelly Fraction  | 1%    |
| P_net Threshold | 0.3%  |
| Profit Reserve  | 40%   |

## Setup

```bash
cp .env.example .env
# Edit .env with your values

cargo build --release
cargo run --release
```

### Docker

```bash
docker build -t l2-arb-engine .
docker run -p 8080:8080 --env-file .env l2-arb-engine
```

## HTTP API

| Endpoint  | Method | Description    |
|-----------|--------|----------------|
| `/health` | GET    | Liveness check |
| `/status` | GET    | Engine state   |
| `/mode`   | GET    | Current mode   |

## License

MIT
