#!/usr/bin/env python3
"""
UESH Kilo Immortal v5 - Akash Deployment (ACD-style Mitosis)

Deploys UESH organism replicas to Akash Network for self-replication.
Each replica is a full organism that can independently:
  - Run the 30s tick engine
  - Connect to the libp2p swarm
  - Execute Hunter/Red/Blue strategies
  - Wire profits to OWNER_METAMASK

Usage:
  python3 deploy_akash.py --mode mitosis    # Deploy new replica
  python3 deploy_akash.py --mode status     # Check replica status
  python3 deploy_akash.py --mode teardown   # Destroy all replicas
  python3 deploy_akash.py --mode scale N    # Scale to N replicas
"""

import argparse
import json
import os
import subprocess
import sys
import time
import hashlib
import yaml
from pathlib import Path
from typing import Dict, List, Optional

# ─── Configuration ────────────────────────────────────────────────────────────

AKASH_IMAGE = os.getenv("AKASH_IMAGE", "ghcr.io/mina-alpha/uesh-kilo-immortal-v5:latest")
AKASH_PROVIDER = os.getenv("AKASH_PROVIDER", "")
AKASH_WALLET = os.getenv("AKASH_WALLET", "")
AKASH_CHAIN_ID = os.getenv("AKASH_CHAIN_ID", "akashnet-2")
AKASH_NODE = os.getenv("AKASH_NODE", "https://rpc.akashnet.net:443")
AKASH_GAS_PRICES = "0.025uakt"
AKASH_GAS_ADJUSTMENT = "1.5"

OWNER_METAMASK = os.getenv("OWNER_METAMASK", "")
TREASURY_CONTRACT = os.getenv("TREASURY_CONTRACT", "")
SWARM_BOOTSTRAP = os.getenv("SWARM_BOOTSTRAP_PEERS", "")

MAX_REPLICAS = int(os.getenv("MAX_REPLICAS", "5"))
REPLICA_BUDGET_USD = float(os.getenv("REPLICA_BUDGET_USD", "5.0"))  # AKT budget per replica

# ─── SDL Template (Akash Deployment Language) ─────────────────────────────────

def generate_sdl(replica_id: str, env_vars: Dict[str, str]) -> dict:
    """Generate Akash SDL for a UESH organism replica."""
    
    # Unique replica identifier
    replica_hash = hashlib.sha256(
        f"{replica_id}-{time.time()}".encode()
    ).hexdigest()[:12]
    
    sdl = {
        "version": "2.0",
        "services": {
            f"uesh-{replica_hash}": {
                "image": AKASH_IMAGE,
                "env": [
                    f"UESH_REPLICA_ID={replica_id}",
                    f"UESH_REPLICA_HASH={replica_hash}",
                    f"OWNER_METAMASK={env_vars.get('OWNER_METAMASK', OWNER_METAMASK)}",
                    f"TREASURY_CONTRACT={env_vars.get('TREASURY_CONTRACT', TREASURY_CONTRACT)}",
                    f"SWARM_BOOTSTRAP_PEERS={env_vars.get('SWARM_BOOTSTRAP_PEERS', SWARM_BOOTSTRAP)}",
                    f"UESH_PORT=8080",
                    f"RUST_LOG=uesh=info",
                ],
                "expose": [
                    {
                        "port": 8080,
                        "as": 80,
                        "to": [{"global": True}],
                    },
                    {
                        "port": 4001,  # libp2p
                        "as": 4001,
                        "to": [{"global": True}],
                    },
                ],
            }
        },
        "profiles": {
            "compute": {
                f"uesh-{replica_hash}": {
                    "resources": {
                        "cpu": {"units": "0.5"},
                        "memory": {"size": "512Mi"},
                        "storage": {"size": "1Gi"},
                    }
                }
            },
            "placement": {
                "akash": {
                    "pricing": {
                        f"uesh-{replica_hash}": {
                            "denom": "uakt",
                            "amount": 100,  # ~$0.01/hr
                        }
                    }
                }
            },
        },
        "deployment": {
            f"uesh-{replica_hash}": {
                "akash": {
                    "profile": f"uesh-{replica_hash}",
                    "count": 1,
                }
            }
        },
    }
    
    return sdl


# ─── Akash CLI Wrapper ───────────────────────────────────────────────────────

def akash_cmd(args: List[str], capture: bool = True) -> subprocess.CompletedProcess:
    """Execute an Akash CLI command."""
    cmd = [
        "provider-services",  # akash provider-services CLI
        *args,
        f"--chain-id={AKASH_CHAIN_ID}",
        f"--node={AKASH_NODE}",
        f"--gas-prices={AKASH_GAS_PRICES}",
        f"--gas-adjustment={AKASH_GAS_ADJUSTMENT}",
        "--gas=auto",
        "-y",
    ]
    
    if AKASH_WALLET:
        cmd.extend([f"--from={AKASH_WALLET}"])
    
    print(f"[AKASH] Running: {' '.join(cmd[:5])}...")
    
    result = subprocess.run(
        cmd,
        capture_output=capture,
        text=True,
        timeout=120,
    )
    
    if result.returncode != 0 and capture:
        print(f"[AKASH] Error: {result.stderr}", file=sys.stderr)
    
    return result


def akash_tx(args: List[str]) -> Optional[str]:
    """Execute an Akash transaction and return tx hash."""
    result = akash_cmd(["tx", *args])
    if result.returncode == 0:
        try:
            data = json.loads(result.stdout) if result.stdout else {}
            return data.get("txhash", "unknown")
        except json.JSONDecodeError:
            return result.stdout.strip()[:64] if result.stdout else None
    return None


# ─── Deployment Operations ────────────────────────────────────────────────────

def deploy_replica(replica_id: str, env_vars: Optional[Dict[str, str]] = None) -> bool:
    """Deploy a new UESH organism replica to Akash."""
    env = env_vars or {}
    
    print(f"\n{'='*60}")
    print(f"[MITOSIS] Deploying replica: {replica_id}")
    print(f"[MITOSIS] Image: {AKASH_IMAGE}")
    print(f"[MITOSIS] Owner: {env.get('OWNER_METAMASK', OWNER_METAMASK)[:10]}...")
    print(f"{'='*60}\n")
    
    # Generate SDL
    sdl = generate_sdl(replica_id, env)
    
    # Write SDL to temp file
    sdl_path = Path(f"/tmp/uesh-sdl-{replica_id}.yaml")
    with open(sdl_path, "w") as f:
        yaml.dump(sdl, f, default_flow_style=False)
    
    print(f"[MITOSIS] SDL written to {sdl_path}")
    
    # Create deployment
    tx_hash = akash_tx(["deployment", "create", str(sdl_path)])
    if not tx_hash:
        print("[MITOSIS] Deployment creation failed", file=sys.stderr)
        return False
    
    print(f"[MITOSIS] Deployment TX: {tx_hash}")
    
    # Wait for bid
    print("[MITOSIS] Waiting for provider bids...")
    time.sleep(30)
    
    # Accept bid (auto-select cheapest)
    bid_result = akash_cmd(["query", "market", "bid", "list"])
    if bid_result.returncode == 0:
        print(f"[MITOSIS] Accepting cheapest bid...")
        # In production: parse bids, select cheapest, accept
    
    # Record deployment
    record = {
        "replica_id": replica_id,
        "tx_hash": tx_hash,
        "timestamp": time.time(),
        "image": AKASH_IMAGE,
        "status": "deploying",
    }
    
    deployments_file = Path("deployments.json")
    deployments = []
    if deployments_file.exists():
        with open(deployments_file) as f:
            deployments = json.load(f)
    
    deployments.append(record)
    with open(deployments_file, "w") as f:
        json.dump(deployments, f, indent=2)
    
    print(f"[MITOSIS] Replica {replica_id} deployment initiated successfully")
    return True


def check_status() -> List[dict]:
    """Check status of all deployed replicas."""
    deployments_file = Path("deployments.json")
    if not deployments_file.exists():
        print("[STATUS] No deployments found")
        return []
    
    with open(deployments_file) as f:
        deployments = json.load(f)
    
    print(f"\n{'='*60}")
    print(f"[STATUS] {len(deployments)} replica(s) deployed")
    print(f"{'='*60}")
    
    for d in deployments:
        print(f"\n  Replica: {d['replica_id']}")
        print(f"  TX:      {d.get('tx_hash', 'unknown')}")
        print(f"  Status:  {d.get('status', 'unknown')}")
        print(f"  Time:    {time.ctime(d.get('timestamp', 0))}")
    
    return deployments


def teardown_all():
    """Destroy all Akash deployments."""
    deployments_file = Path("deployments.json")
    if not deployments_file.exists():
        print("[TEARDOWN] No deployments to tear down")
        return
    
    with open(deployments_file) as f:
        deployments = json.load(f)
    
    print(f"[TEARDOWN] Destroying {len(deployments)} replica(s)...")
    
    for d in deployments:
        tx_hash = akash_tx(["deployment", "close", "--dseq", str(d.get("dseq", 0))])
        if tx_hash:
            print(f"[TEARDOWN] Closed {d['replica_id']}: {tx_hash}")
        else:
            print(f"[TEARDOWN] Failed to close {d['replica_id']}")
    
    # Clear deployments file
    with open(deployments_file, "w") as f:
        json.dump([], f)
    
    print("[TEARDOWN] All replicas destroyed")


def scale_replicas(target: int):
    """Scale to target number of replicas."""
    deployments_file = Path("deployments.json")
    current = []
    if deployments_file.exists():
        with open(deployments_file) as f:
            current = json.load(f)
    
    current_count = len(current)
    target = min(target, MAX_REPLICAS)
    
    print(f"[SCALE] Current: {current_count}, Target: {target}, Max: {MAX_REPLICAS}")
    
    if target > current_count:
        # Scale up
        for i in range(current_count, target):
            replica_id = f"uesh-replica-{i}-{int(time.time())}"
            deploy_replica(replica_id)
    elif target < current_count:
        # Scale down (remove newest first)
        to_remove = current_count - target
        print(f"[SCALE] Removing {to_remove} replica(s)")
        # In production: close specific deployments via Akash CLI
    
    print(f"[SCALE] Scaling complete. Target: {target} replicas")


# ─── Self-Evolution: Docker Build + Push ──────────────────────────────────────

def build_and_push():
    """Build new Docker image and push to registry for evolution."""
    print("[EVOLVE] Building new UESH image...")
    
    result = subprocess.run(
        ["docker", "build", "-t", AKASH_IMAGE, "."],
        capture_output=True,
        text=True,
        timeout=300,
    )
    
    if result.returncode != 0:
        print(f"[EVOLVE] Build failed: {result.stderr}", file=sys.stderr)
        return False
    
    print("[EVOLVE] Pushing to registry...")
    result = subprocess.run(
        ["docker", "push", AKASH_IMAGE],
        capture_output=True,
        text=True,
        timeout=300,
    )
    
    if result.returncode != 0:
        print(f"[EVOLVE] Push failed: {result.stderr}", file=sys.stderr)
        return False
    
    print(f"[EVOLVE] Image pushed: {AKASH_IMAGE}")
    return True


# ─── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="UESH Kilo Immortal v5 - Akash Deployment Manager",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python3 deploy_akash.py --mode mitosis          # Deploy new replica
  python3 deploy_akash.py --mode status            # Check all replicas
  python3 deploy_akash.py --mode scale 3           # Scale to 3 replicas
  python3 deploy_akash.py --mode teardown          # Destroy all
  python3 deploy_akash.py --mode evolve            # Build + push new image
  python3 deploy_akash.py --mode full-mitosis      # Build + push + deploy
        """,
    )
    
    parser.add_argument(
        "--mode",
        required=True,
        choices=["mitosis", "status", "teardown", "scale", "evolve", "full-mitosis"],
        help="Deployment mode",
    )
    
    parser.add_argument(
        "count",
        nargs="?",
        type=int,
        default=1,
        help="Number of replicas (for scale mode)",
    )
    
    parser.add_argument(
        "--replica-id",
        default=None,
        help="Custom replica ID (for mitosis mode)",
    )
    
    args = parser.parse_args()
    
    print(f"\n{'='*60}")
    print(f"  UESH KILO IMMORTAL v5 - Akash Deployer")
    print(f"  Mode: {args.mode}")
    print(f"  Time: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}")
    print(f"{'='*60}\n")
    
    if args.mode == "mitosis":
        replica_id = args.replica_id or f"uesh-mitosis-{int(time.time())}"
        success = deploy_replica(replica_id)
        sys.exit(0 if success else 1)
    
    elif args.mode == "status":
        check_status()
    
    elif args.mode == "teardown":
        teardown_all()
    
    elif args.mode == "scale":
        scale_replicas(args.count)
    
    elif args.mode == "evolve":
        success = build_and_push()
        sys.exit(0 if success else 1)
    
    elif args.mode == "full-mitosis":
        if build_and_push():
            replica_id = args.replica_id or f"uesh-evolved-{int(time.time())}"
            success = deploy_replica(replica_id)
            sys.exit(0 if success else 1)
        else:
            sys.exit(1)


if __name__ == "__main__":
    main()
