# Security

Report vulnerabilities privately via GitHub Security Advisories on [cgfixit/Mac-NetViewer-EZview](https://github.com/cgfixit/Mac-NetViewer-EZview/security/advisories/new). Do not file a public issue for a still-unfixed hole.

This tool lists other processes' sockets on purpose. Do not enable App Sandbox. There is no bounty and no SLA.

`cargo audit` runs weekly on `Cargo.lock`. Rustc is pinned at 1.85.0. Do not float `eframe` without checking that pin still builds.
