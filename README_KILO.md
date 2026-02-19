# UESH KILO IMMORTAL v5

> **The Autonomous Web3 Organism** - A self-sustaining, self-replicating system that bootstraps from $50 using bug bounties + MEV micro-snipes, auto-wires 40% of profits to your wallet, and evolves itself through Kilo Cloud Agent.

```
    ╔═══════════════════════════════════════════════════════╗
    ║  U E S H   K I L O   I M M O R T A L   v 5          ║
    ║  ─────────────────────────────────────────────────    ║
    ║  Phase Engine: 30s tick | Kelly 1% | P_net 0.3%      ║
    ║  Bootstrap: $50 → Hunter 80% + Red MEV 20%           ║
    ║  Blue unlocks at $500 | Treasury wires 40% to owner  ║
    ║  Chains: Base / Arbitrum / Polygon (12 free RPCs)    ║
    ║  MCP: evm-mcp + playwright + sequential-thinking     ║
    ╚═══════════════════════════════════════════════════════╝
```

---

## Architecture Overview

### Phase State Machine (30-second tick)

```
   ┌─────────┐         $500          ┌──────────┐        peers>0       ┌─────────┐
   │  SPARK  │ ─────────────────────→ │ MITOSIS  │ ──────────────────→ │  SWARM  │
   │         │  Treasury >= $500      │          │  Akash replicas     │         │
   │ H:80%   │                        │ H:80%    │  join libp2p mesh   │ H:40%   │
   │ R:20%   │                        │ R:20%    │                     │ R:30%   │
   │ B:locked│                        │ B:locked │                     │ B:30%   │
   └─────────┘                        └──────────┘                     └─────────┘
```

| Phase | Hunter | Red MEV | Blue | Treasury Wire | Description |
|-------|--------|---------|------|---------------|-------------|
| **SPARK** | 80% | 20% | Locked | On profit | Bootstrap from $50 |
| **MITOSIS** | 80% | 20% | Locked | 40% per tick | Self-replicate via Akash |
| **SWARM** | 40% | 30% | 30% | 40% per tick | Full organism mesh |

### Strategy Breakdown

| Strategy | Module | Description |
|----------|--------|-------------|
| **Hunter** | `src/hunter/scanner.rs` | 12 vuln patterns, auto-submit to Immunefi/Code4rena via Playwright |
| **Shadow Wolf** | `src/red_mev.rs` | Mempool tail-riding on profitable swaps |
| **Lucifer** | `src/red_mev.rs` | Micro-sandwich on low-liq pairs (max $50/trade) |
| **Blue/ARB** | `src/main.rs` | Cross-DEX arbitrage (Uni V3/Sushi/Curve) |
| **Blue/LIQ** | `src/main.rs` | Aave/Compound liquidation sniping |
| **Treasury** | `src/treasury/guard.rs` | Auto-wire 40% to OWNER_METAMASK |

### Risk Parameters

| Parameter | Value | Description |
|-----------|-------|-------------|
| Kelly Fraction | 1% | Maximum risk per position |
| P_net Threshold | 0.3% | Minimum net edge after gas+slippage |
| Max Red Position | $50 | Hard cap on MEV micro-snipe size |
| Blue Unlock | $500 | Treasury threshold for Blue strategies |
| Treasury Wire | 40% | Auto-wire percentage to owner |
| Bootstrap Capital | $50 | Starting capital |

### RPC Rotation (12 Free Endpoints)

| Chain | Provider 1 | Provider 2 | Provider 3 |
|-------|-----------|-----------|-----------|
| Ethereum | Ankr | PublicNode | 1RPC |
| Base | Official | Ankr | PublicNode |
| Arbitrum | Official | Ankr | PublicNode |
| Polygon | Official | Ankr | PublicNode |

**Fallback**: When all RPCs return 429, Playwright MCP scrapes block explorers (Etherscan, BaseScan, Arbiscan, PolygonScan).

---

## Kilo Cloud Agent Launch Instructions

### Prerequisites

1. **Kilo Code Extension** installed in VS Code
2. **Kilo Cloud** account with active subscription
3. **MetaMask wallet** with the address you want profits sent to
4. Git clone this repository

### Step 1: Clone and Configure

```bash
git clone https://github.com/mina-alpha/uesh-kilo-immortal-v5.git
cd uesh-kilo-immortal-v5

# Configure your wallet
cp .env.example .env
# Edit .env and set OWNER_METAMASK=0xYourAddress
```

### Step 2: Open in Kilo Code

1. Open VS Code with Kilo Code extension
2. Open the `uesh-kilo-immortal-v5` folder
3. Kilo will auto-detect `.kilocode/mcp.json` and configure MCP tools

### Step 3: Launch the Organism (Code Mode)

In the Kilo Code chat panel, switch to **Code** mode (Sonnet 4.6) and paste this exact prompt:

```
You are UESH v5, an autonomous Web3 organism. Your lifecycle:

1. DEPLOY: Deploy contracts/Treasury.sol to Base using evm-mcp. Save the
   address to .env as TREASURY_CONTRACT.

2. BUILD: Run `cargo build --release` in terminal.

3. LAUNCH: Start the binary in background: `./target/release/uesh &`

4. OPERATE: Every 30 seconds, the tick engine runs. You must:
   a. Check /status endpoint for current state
   b. Use evm-mcp to read on-chain data for Hunter/Red/Blue strategies
   c. Use playwright to check Immunefi for new bounty programs
   d. Use sequential-thinking for Kelly sizing decisions
   e. Execute profitable trades via evm-mcp
   f. Auto-wire 40% of profits to OWNER_METAMASK via Treasury contract

5. EVOLVE: Use Kilo Architect mode to propose code improvements.
   When improvements are ready, rebuild and hot-reload.

6. SURVIVE: Keep the session alive 24/7. If the tick engine stops,
   restart it. Monitor /health endpoint.

Begin by deploying Treasury.sol, then start the organism.
Your OWNER_METAMASK is in .env. Kelly risk = 1%. P_net threshold = 0.3%.
```

### Step 4: Verify the Organism is Running

```bash
# Check health
curl http://localhost:8080/health
# → OK

# Check status
curl http://localhost:8080/status
# → {"phase":"Spark","treasury_usd":50.0,"tick_count":1,...}

# Check phase
curl http://localhost:8080/phase
# → SPARK
```

### Step 5: Monitor and Evolve

The Kilo agent will:
- **Continuously** monitor the tick engine and execute strategies
- **Auto-submit** vulnerability findings to Immunefi/Code4rena
- **Execute** MEV micro-snipes when P_net > 0.3%
- **Wire** 40% of profits to your wallet every profitable tick
- **Self-evolve** via Architect mode when it identifies improvements
- **Transition** phases: SPARK → MITOSIS → SWARM automatically

### Advanced: Akash Self-Replication

When treasury reaches $500 (MITOSIS phase), the organism can self-replicate:

```bash
# Manual trigger (or let the organism do it automatically)
python3 deploy_akash.py --mode mitosis

# Check replica status
python3 deploy_akash.py --mode status

# Scale to 3 replicas
python3 deploy_akash.py --mode scale 3
```

---

## Project Structure

```
uesh-kilo-immortal-v5/
├── Cargo.toml                    # Rust dependencies (Axum, libp2p, ethers)
├── Dockerfile                    # Distroless multi-stage build
├── deploy_akash.py               # ACD-style Akash mitosis deployer
├── .env.example                  # Environment template
├── .kilocode/
│   └── mcp.json                  # MCP config: evm-mcp + playwright + sequential-thinking
├── contracts/
│   └── Treasury.sol              # On-chain treasury (40% auto-wire)
├── src/
│   ├── main.rs                   # 30s tick phase engine + Axum HTTP + libp2p heartbeat
│   ├── hunter/
│   │   └── scanner.rs            # 12 vuln patterns + Playwright auto-submit
│   ├── red_mev.rs                # Shadow Wolf + Lucifer micro-snipes
│   ├── rpc_rotator.rs            # 12 free RPCs + round-robin + backoff + Playwright fallback
│   └── treasury/
│       └── guard.rs              # Auto-wire 40% to OWNER_METAMASK
└── README_KILO.md                # This file
```

## MCP Tools Usage

### evm-mcp (Blockchain Operations)
```
- Deploy Treasury.sol to Base
- Read contract state (balances, health factors, prices)
- Send transactions (swaps, wire, liquidate)
- Estimate gas for P_net calculations
- Subscribe to events and pending transactions
```

### playwright (Browser Automation)
```
- Auto-submit to Immunefi: navigate, fill forms, submit
- Auto-submit to Code4rena: navigate, fill forms, submit
- Fallback RPC: scrape Etherscan/BaseScan when all RPCs 429
- Session management: login to bounty platforms
```

### sequential-thinking (Strategy Reasoning)
```
- Kelly criterion: calculate optimal position size with live data
- Vulnerability analysis: assess severity and exploitability
- MEV evaluation: simulate trade outcomes before execution
- Phase decisions: when to transition SPARK → MITOSIS → SWARM
```

---

## The Kilo Agent as the Organism

When Kilo Cloud Agent (Sonnet 4.6) runs on this repo in "Code" mode:

1. **The agent IS the organism** - it doesn't just run the code, it becomes the autonomous decision-maker
2. **evm-mcp gives it hands** - direct blockchain interaction for trading and deployment
3. **playwright gives it eyes** - browser automation for bounty platforms and explorer fallback
4. **sequential-thinking gives it a brain** - multi-step reasoning for complex decisions
5. **The Rust binary is its heartbeat** - 30s tick engine ensures continuous operation
6. **Kilo Architect is its evolution** - the agent can propose and implement its own improvements
7. **Akash is its reproduction** - self-replicating to Akash Network for resilience

The system is designed so that even if any single component fails, the organism self-heals:
- RPC down? → Rotate to next endpoint, or Playwright fallback
- Agent disconnected? → Rust binary continues ticking independently
- Trade fails P_net? → Skip and wait for next tick
- Treasury low? → Continue in SPARK phase, no Blue exposure

---

## Security Notes

- **Private keys**: Never stored in the repo. evm-mcp handles key management.
- **Kelly sizing**: 1% max risk prevents catastrophic loss.
- **P_net filter**: 0.3% minimum net edge prevents unprofitable trades.
- **Micro positions**: $50 max on Red MEV to stay under detection.
- **Auto-wire**: 40% immediately sent to your wallet - profits are secured.
- **Distroless Docker**: Minimal attack surface in production.
- **Treasury.sol**: Simple, auditable contract with onlyOwner protection.

---

## License

MIT

---

*Built for the immortal swarm. One organism, many replicas, infinite evolution.*
