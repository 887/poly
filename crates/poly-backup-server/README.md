# poly-backup-server

Encrypted settings backup/sync server for **Poly** (PolyGlot Messenger).

## Purpose

A standalone server that stores encrypted copies of Poly app settings. Users can configure multiple backup servers for redundancy. The server is privacy-first — it stores only encrypted blobs and identifies users by their Ed25519 public key.

## Features

- **Zero-knowledge storage**: Server sees only encrypted blobs, never plaintext
- **Proof-of-work auth**: Anubis-style PoW challenge prevents brute-force
- **Server passphrase**: Only users with the passphrase can register
- **Account limit**: Configurable maximum number of accounts
- **Session tokens**: Long-lived tokens with device tracking and inactivity expiry
- **Sync protocol**: Push/pull encrypted settings with sequence numbers
- **Admin web UI**: Manage accounts, sessions, server configuration
- **REST API**: Full CRUD for sync operations

## Running

```bash
POLY_PASSPHRASE="your-secret" cargo run -p poly-backup-server
```

## Configuration

| Variable | Default | Description |
|---|---|---|
| `POLY_PASSPHRASE` | (required) | Server access passphrase |
| `POLY_MAX_ACCOUNTS` | `0` | Max accounts (0 = unlimited) |
| `POLY_TOKEN_EXPIRY_DAYS` | `365` | Token inactivity expiry |
| `POLY_POW_DIFFICULTY` | `20` | PoW challenge difficulty |
| `POLY_BIND_ADDRESS` | `0.0.0.0:3000` | Listen address |

## License

MIT / Apache-2.0
