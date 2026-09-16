# Vendored specmark provenance

The ZAP Rust workspace vendors the two inert specmark crates required by its
source annotations so an installed package builds from stable package-local
paths. They were copied byte-for-byte on 2026-09-14 from the materialized
package `lang:org.vibevm.ai-native/rust-ai-native-lang@=1.0.0`, whose locked
package identity at import was
`sha256:ddfe01d8b1b7c2694ef6409276873c10b6f51fe98c14faf774dbaadde0aa8ab4`.

Both upstream crate manifests inherit UPL-1.0 from that package. ZAP is also
UPL-1.0; the complete license text is at the ZAP package root. The copied crate
files are unchanged. ZAP's root workspace supplies only the inherited package
metadata and workspace dependency paths they require.

## core-ai-native-specmark

| Relative path | SHA-256 |
| --- | --- |
| `Cargo.toml` | `17444834b7122cca8ef8dcb0102b343b02817c4ace716d6084c84460fa7b3b17` |
| `src/lib.rs` | `b3b47781e0cc6c119603ffbc28f22449a9a3fa1bf01e80c6bdb1b454b36d91cb` |
| `tests/usage.rs` | `52598f168e29028d748f632371dafdbff9e6aa1f6a1086cf42dcd9c6ba374359` |

## core-ai-native-specmark-grammar

| Relative path | SHA-256 |
| --- | --- |
| `Cargo.toml` | `d13d5bc78493be32a8990b2a1fc8f6f83e8fb2e2c2a99494255675c3b55f2e05` |
| `src/lib.rs` | `7ec35f63d71ebd7b4981f4dd9c6ea47ce75767f8493014a3d4c89adddc328c01` |
| `src/lib/tests.rs` | `9407fbc9b6d73e941cc163febfb2b4b805a0e9e47fec0cae6f832d8e489c70f1` |

To refresh these copies, update the declared Rust AI Native package first,
copy the same two complete crate directories, recompute this table, and verify
the copied files before changing the recorded upstream package identity.
