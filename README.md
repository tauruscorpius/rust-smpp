# rust-smpp

An [SMPP](https://smpp.org/) library for Rust.

In early development: a basic framework for an SMSC is done, but many features
required for a robust service are missing.  See
[Issues](https://gitlab.com/andybalaam/rust-smpp/-/issues).

See also: [API docs](https://docs.rs/smpp) and
[crates.io entry](https://crates.io/crates/smpp)

## Server application (SMSC)

First, [install Rust](https://www.rust-lang.org/tools/install).

To launch an SMSC:

```bash
cargo run
```

To find parameters that you can provide on startup, run:

```bash
cargo run -- --help
```

`--` is what separates cargo arguments and binary arguments, so to 
provide a different bind address, run:

```bash
cargo run -- -b 0.0.0.0:23432
```

You can also provide parameters as environment variables as described in help:

```bash
BIND_ADDRESS=0.0.0.0:23432 cargo run
```

To launch with detailed logging:

```bash
RUST_LOG=DEBUG cargo run
```

## Publishing releases

```bash
cargo update
vim CHANGELOG.md   # Set the version number
vim Cargo.toml     # Set the version number
cargo publish
git tag $VERSION
git push --tags
```

## Reference documentation

Development focusses on SMPP v3.4, since that is in wide use.  Docs:

* [SMPP Spec v3.4 Issue 1.2](https://smpp.org/SMPP_v3_4_Issue1_2.pdf)
* [SMPP v3.4 Implementation Guide v1.0](https://smpp.org/smppv34_gsmumts_ig_v10.pdf)
* [How to send an SMS using netcat](https://www.artificialworlds.net/blog/2020/08/10/how-to-send-an-sms-using-netcat-via-smpp/)

## Code of conduct

We follow the [Rust code of conduct](https://www.rust-lang.org/conduct.html).

Currently the moderation team consists of Andy Balaam only.  We would welcome
more members: if you would like to join the moderation team, please contact
Andy Balaam.

Andy Balaam may be contacted by email on andybalaam at artificialworlds.net or
on mastodon on
[@andybalaam@mastodon.social](https://mastodon.social/web/accounts/7995).

## License

rust-smpp is distributed under the terms of both the [MIT license](LICENSE-MIT)
and the [Apache License (Version 2.0)](LICENSE-APACHE).

This project is developed in my personal time. A previous employer (OpenMarket)
allowed me to develop it on work time, and release it under my personal
copyright.
