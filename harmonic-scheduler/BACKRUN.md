# Co-located pre-TPU transaction stream

This patch lets a Harmonic validator stream its **own** pre-execution
transactions to a co-located strategy server — but **only while this validator
is the leader**. It is **one-way and read-only**: it does not accept
transactions back and does not modify block production, so it does not interfere
with Harmonic's auction or block builder. The data is the validator's own
transaction ingress.

The feature is **opt-in**. A validator that updates to this build but does not
pass `--backrun-listen-addr` behaves exactly as before.

## Updating an existing 4.0.3 validator

This patch changes **only the `harmonic-scheduler` crate**. The agave validator
binary, `scheduler-bindings`, the IPC surface, and on-disk state are all
unchanged, so updating is just swapping the scheduler binary:

1. Check out this branch and rebuild only the scheduler:
   ```bash
   cargo build --release -p harmonic-scheduler
   ```
2. Replace your running `harmonic-scheduler` binary with the new
   `target/release/harmonic-scheduler`.
3. Restart the scheduler, adding `--backrun-listen-addr`. Every other flag
   stays the same.

No change to the agave validator process, no re-sync, no gossip/config changes.
Rolling back is the same step in reverse (old binary, drop the flag).

## Connection model

By gRPC role the **validator is the server** and your **strategy box is the
client** — even though the strategy is the one "listening" for transactions.
The validator has the transactions, so it hosts the server and streams them out
over `SubscribeBackruns`; the strategy dials in and subscribes.

The connection is **persistent**:

- The server is started once at process startup and listens for the whole
  lifetime of the scheduler, independent of its IPC reconnect loop.
- The client opens a single long-lived `SubscribeBackruns` stream that is
  **never closed based on leader state**. While not leader it carries only 5s
  keepalive pings; while leader it carries transactions. So the stream stays
  warm across slots — there is no per-slot connect/handshake latency.
- Streaming is gated on leadership *at the data level only* (the `is_leader`
  flag), not at the connection level.

The strategy should treat the stream as long-lived and **reconnect on drop**
(e.g. validator restart). The `searcher-client-example` client already does this.

## How it works

```
            leader slot only
  validator TPU ─┐
                 ├─► nonvote ingress ──► (stream out) ──► strategy server
  relayer ───────┘                                         SubscribeBackruns
```

- **Stream out** (`validator/tpu.rs`): the scheduler already funnels every
  incoming non-vote transaction — both the validator's direct TPU stream
  (`tpu_to_pack`) and its relayer-forwarded packets — through one point. While
  the `is_leader` flag is set, each such transaction's raw bytes are mirrored to
  the `BackrunService` subscribers. Votes are not streamed.
- **Leader gating** (`validator/scheduler.rs`): the scheduler sets `is_leader`
  true on entering a leader slot and false otherwise. Outside the leader slot,
  nothing is streamed.

Nothing is written back into block production. The stream is a copy of the
ingress path; it does not change which transactions the validator executes or
the order it executes them in.

The gRPC server (`backrun.rs`) implements the `BackrunService` that
`searcher-client-example-master` speaks, so the strategy server connects as the
client with no changes.

## Running

Start the scheduler with the stream endpoint enabled:

```bash
harmonic-scheduler \
  --ledger /path/to/ledger \
  --remote-tpu-url <REMOTE_TPU_URL> \
  --block-engine-url <BLOCK_ENGINE_URL> \
  --tip-payment-program-pubkey <PUBKEY> \
  --tip-distribution-program-pubkey <PUBKEY> \
  --merkle-root-upload-authority <PUBKEY> \
  --backrun-listen-addr 0.0.0.0:50051        # enables the feature
```

The strategy server (co-located, same datacenter) connects with the
`searcher-client-example` client — just the IP and port, no token:

```bash
backrun_grpc_client --grpc-url http://<validator-host>:50051
```

It receives every non-vote transaction the validator sees during its leader
slots. What you do with that stream is up to your strategy and lives entirely on
your side.

There is **no authentication** on the endpoint — restrict access at the network
layer (the bind address and your firewall).

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--backrun-listen-addr` | unset (disabled) | Address to serve the pre-TPU gRPC stream on |

## Notes

- Streaming is active **only during this validator's leader slots**.
- This build does **not** submit transactions or bundles back to the validator.
  It is intentionally read-only: writing into the block while Harmonic's builder
  is producing it would cause a state mismatch with the builder (block
  divergence / unbundling). To act on these transactions you need a TPU/auction-
  side integration with Harmonic, not a scheduler-side write.
