# Changelog

## [0.12.1](https://github.com/steveyackey/devrig/compare/v0.12.0...v0.12.1) (2026-02-24)


### Bug Fixes

* resolve relative bind mount paths to absolute for Docker ([8c8401b](https://github.com/steveyackey/devrig/commit/8c8401bb097a6a9eed7817b50521100c4d44943d))

## [0.12.0](https://github.com/steveyackey/devrig/compare/v0.11.0...v0.12.0) (2026-02-24)


### Features

* add `devrig skill reference` command and slim down SKILL.md ([03e5c8f](https://github.com/steveyackey/devrig/commit/03e5c8f09fc7b041b925b28c30a35cbe4d6602d1))

## [0.11.0](https://github.com/steveyackey/devrig/compare/v0.10.1...v0.11.0) (2026-02-24)


### Features

* add 5 cluster/addon features and fix startup table border ([dcc05b3](https://github.com/steveyackey/devrig/commit/dcc05b32d1119a1ba392c12ad168bd53b46548c1))


### Bug Fixes

* remove flaky waitForResponse from logs clear button E2E test ([607ab5e](https://github.com/steveyackey/devrig/commit/607ab5eadfdc5bc6723ea446ad7db667ddccb7b0))

## [0.10.1](https://github.com/steveyackey/devrig/compare/v0.10.0...v0.10.1) (2026-02-24)


### Bug Fixes

* suppress internal orchestration logs from terminal output ([24e8a98](https://github.com/steveyackey/devrig/commit/24e8a9800c2b0073b813b7aab6ba170408877ddd))

## [0.10.0](https://github.com/steveyackey/devrig/compare/v0.9.0...v0.10.0) (2026-02-24)


### Features

* show exit codes on dashboard, full-width status bar, and E2E test improvements ([7c9f6a8](https://github.com/steveyackey/devrig/commit/7c9f6a81ed7c157aa98d0e7292ad8f5c520b79e5))
* start dashboard before infrastructure and show yellow for starting services ([6ee1505](https://github.com/steveyackey/devrig/commit/6ee1505ae31a2af86df331166530d1f38e295dd1))


### Bug Fixes

* prevent e2e test hangs with timeout and config path fix ([#22](https://github.com/steveyackey/devrig/issues/22)) ([e4fb491](https://github.com/steveyackey/devrig/commit/e4fb49122f6a831ef572e311e995c35e1d12ca9d))

## [0.9.0](https://github.com/steveyackey/devrig/compare/v0.8.0...v0.9.0) (2026-02-24)


### Features

* add --all flag to stop/delete and improve Ctrl+C handling ([6121ea4](https://github.com/steveyackey/devrig/commit/6121ea43640be740c948f36f6e1321d4f49f44e1))
* add command and entrypoint overrides for docker containers ([#21](https://github.com/steveyackey/devrig/issues/21)) ([11b57e5](https://github.com/steveyackey/devrig/commit/11b57e58adea2ee56573893480a3ee38d38f40af))
* migrate E2E tests from Playwright runner to bun test + add favicon ([98067e7](https://github.com/steveyackey/devrig/commit/98067e744fcb6445d9fd0d81172888d2ab76c2ed))

## [0.8.0](https://github.com/steveyackey/devrig/compare/v0.7.0...v0.8.0) (2026-02-24)


### Features

* add bind mount support for docker volumes ([832d286](https://github.com/steveyackey/devrig/commit/832d286c98418b14098472735eef83b1494f95a9))
* support local helm charts, values_files, and fix kustomize path resolution ([e9fd43a](https://github.com/steveyackey/devrig/commit/e9fd43a6cd3ce3e1e7d0e2a0d55e855eab07e40a))
* support local helm charts, values_files, and fix kustomize path resolution ([96d767f](https://github.com/steveyackey/devrig/commit/96d767f25160dbc2492928fd3f0c0c6ab59b3fe3))

## [0.7.0](https://github.com/steveyackey/devrig/compare/v0.6.0...v0.7.0) (2026-02-24)


### Features

* show dashboard port in startup summary and ps --all ([2b54373](https://github.com/steveyackey/devrig/commit/2b543735a53e0ef02f65ed5f38444b891ceda0c8))

## [0.6.0](https://github.com/steveyackey/devrig/compare/v0.5.0...v0.6.0) (2026-02-23)


### Features

* stream Docker container logs to dashboard ([5026f50](https://github.com/steveyackey/devrig/commit/5026f504b86b3212df92ce9b3c9474c322fd818b))

## [0.5.0](https://github.com/steveyackey/devrig/compare/v0.4.1...v0.5.0) (2026-02-23)


### Features

* add [cluster.image.*] for build-only images ([1d27b8f](https://github.com/steveyackey/devrig/commit/1d27b8f29f828ddabd7c26f62ec6a070e07f37a0))

## [0.4.1](https://github.com/steveyackey/devrig/compare/v0.4.0...v0.4.1) (2026-02-23)


### Bug Fixes

* add syntax highlighting to config editor ([b48b510](https://github.com/steveyackey/devrig/commit/b48b510d2090e10ab4491105d2c2524f9d111194))
* default to dark mode; enable dashboard in init template ([c34fe1c](https://github.com/steveyackey/devrig/commit/c34fe1c4da469d7af1aa353b584c6dbfc982f9bd))
* enable dashboard by default in devrig init template ([72f7d13](https://github.com/steveyackey/devrig/commit/72f7d132fba19d4354f15827b8b1e7f3f5e70479))
* improve color contrast to meet WCAG AA ([211b56c](https://github.com/steveyackey/devrig/commit/211b56c9f3cd668192c1a20e6cf4417ae5aea15c))
* improve dashboard text contrast and subheader legibility ([719cbbf](https://github.com/steveyackey/devrig/commit/719cbbf5a361c739064d0ffeb262bb0f855d2738))

## [0.4.0](https://github.com/steveyackey/devrig/compare/v0.3.1...v0.4.0) (2026-02-23)


### Features

* split skill into SKILL.md + reference/configuration.md ([091ea01](https://github.com/steveyackey/devrig/commit/091ea01204303d00e3f7f82e7b00d0b56940db67))

## [0.3.1](https://github.com/steveyackey/devrig/compare/v0.3.0...v0.3.1) (2026-02-23)


### Bug Fixes

* auto-resolve dashboard/OTel ports when already in use ([9148e68](https://github.com/steveyackey/devrig/commit/9148e680e007e8fea9313e78ff7159cc496e4c13))

## [0.3.0](https://github.com/steveyackey/devrig/compare/v0.2.1...v0.3.0) (2026-02-23)


### Features

* add secrets management and compose auto-discovery ([4d11521](https://github.com/steveyackey/devrig/commit/4d11521400fe70c7fdb1b05c22a23230621745b3))


### Bug Fixes

* use PATH instead of HOME in env fallback test for Windows compat ([f0afc43](https://github.com/steveyackey/devrig/commit/f0afc43ef8a91056a9432b83990e8280a0587d6e))

## [0.2.1](https://github.com/steveyackey/devrig/compare/v0.2.0...v0.2.1) (2026-02-23)


### Bug Fixes

* allow dirty working tree for crates.io publish ([040ba29](https://github.com/steveyackey/devrig/commit/040ba29432712a798362cbf8f8a1775f6065dd75))

## [0.2.0](https://github.com/steveyackey/devrig/compare/v0.1.5...v0.2.0) (2026-02-23)


### Features

* add `devrig update` command with cargo-dist updater ([154c157](https://github.com/steveyackey/devrig/commit/154c157b384605d0ab8f080b7f679ca959ef93a0))


### Bug Fixes

* build dashboard before crates.io publish so cargo install works ([f137d77](https://github.com/steveyackey/devrig/commit/f137d778e75526939b092443b71611846585ee3d))

## [0.1.5](https://github.com/steveyackey/devrig/compare/v0.1.4...v0.1.5) (2026-02-23)


### Bug Fixes

* correct binstall bin-dir to match cargo-dist archive structure ([7779241](https://github.com/steveyackey/devrig/commit/77792416080f89b21be28919134ee5851250ae2a))

## [0.1.4](https://github.com/steveyackey/devrig/compare/v0.1.3...v0.1.4) (2026-02-23)


### Bug Fixes

* correct binstall pkg-fmt to txz to match cargo-dist archives ([539222f](https://github.com/steveyackey/devrig/commit/539222f4d22c7fc5ef944dd37eccd74775fe3584))

## [0.1.3](https://github.com/steveyackey/devrig/compare/v0.1.2...v0.1.3) (2026-02-23)


### Bug Fixes

* build dashboard frontend in release CI and fix crates.io publish ([4797af1](https://github.com/steveyackey/devrig/commit/4797af127fae8a6fbca099b8544f3ac91533de25))

## [0.1.2](https://github.com/steveyackey/devrig/compare/v0.1.1...v0.1.2) (2026-02-23)


### Bug Fixes

* use crates-io-auth-action for trusted publishing OIDC exchange ([374f50c](https://github.com/steveyackey/devrig/commit/374f50ce2fccdd5b69bf59806f10eff20ce91405))

## [0.1.1](https://github.com/steveyackey/devrig/compare/v0.1.0...v0.1.1) (2026-02-23)


### Bug Fixes

* use PAT for release-please to trigger cargo-dist workflow ([0fe6e94](https://github.com/steveyackey/devrig/commit/0fe6e946e3058449e3e1a1745e4597bf09e3d15e))

## 0.1.0 (2026-02-23)


### Features

* add --dev flag for hot-reload dashboard development ([bea25a3](https://github.com/steveyackey/devrig/commit/bea25a33620d1ddd2843909edc99f726dbb0ae9f))
* add instrumented demo app example (frontend + backend + Postgres) ([41c75d8](https://github.com/steveyackey/devrig/commit/41c75d805c426cc6a7b97bb1131ec614e0bc691a))
* add native Windows support via platform abstraction layer ([45d2553](https://github.com/steveyackey/devrig/commit/45d255399347dc0b0fcea2e5c3034af24bb2705b))
* add PRD-driven agent build pipeline ([47d1127](https://github.com/steveyackey/devrig/commit/47d11270856aaa28e86ddddfeda751c1f63b6cd2))
* add release automation with cargo-dist and release-please ([333eb35](https://github.com/steveyackey/devrig/commit/333eb35ebc18605bb187f618412a7cdeb973aa77))
* auto-publish to crates.io via trusted publishing ([dec6490](https://github.com/steveyackey/devrig/commit/dec6490d5d3d9dc51d35b4e519158b6569cb5d4c))
* cache-busting image hashes in README, add logs/metrics screenshots ([faef379](https://github.com/steveyackey/devrig/commit/faef379e7aeb85bde614986dedadfba9cb3ea067))
* content-hashed screenshot filenames for cache busting ([ecbe7c6](https://github.com/steveyackey/devrig/commit/ecbe7c63b457426b853f456d700e44a12c214a55))
* dashboard spacing, service port links, screenshot test ([8a37dc2](https://github.com/steveyackey/devrig/commit/8a37dc2e8d903a83be632264fd7f06915a02bbd7))
* implement Stencil Yard design theme across dashboard ([8a24df6](https://github.com/steveyackey/devrig/commit/8a24df6a9b99ed2f009b2ebac87d2fefff86bdbf))
* make Status the default landing page and first sidebar item ([bdc06c6](https://github.com/steveyackey/devrig/commit/bdc06c6e8b6099860d6ea0b0840708ad803b00e5))
* **pipeline:** add git commit and push after verified milestones ([470794f](https://github.com/steveyackey/devrig/commit/470794f293d03174dfa630b59e58c3dcfdd6ca60))
* **pipeline:** add targeted fix mode for retry attempts ([bb73d05](https://github.com/steveyackey/devrig/commit/bb73d052c7b4c3cd6abfd248fb741472835a300a))
* unified log collection — bridge process logs to dashboard + k3d Fluent Bit addon ([eefdf94](https://github.com/steveyackey/devrig/commit/eefdf940f398c2045c8b62ec3c8a87e1f7c9ecf1))
* **v0.1:** local process orchestration ([20ac4d9](https://github.com/steveyackey/devrig/commit/20ac4d9cea4b3eb08d358a1ce1f043e0f6a405ed))
* **v0.2:** Docker containers ([43aaf41](https://github.com/steveyackey/devrig/commit/43aaf413fe27d2bb128cbccbc2dc79a72710cf44))
* **v0.3:** k3d cluster support ([a1b64ec](https://github.com/steveyackey/devrig/commit/a1b64ecce6c81cd528eef7126c6b7d65da8b2f4d))
* **v0.4:** Developer experience polish ([943a8c9](https://github.com/steveyackey/devrig/commit/943a8c9577550aacd1b6c320029ecefb1e948201))
* **v0.5:** Observability + Dashboard ([d86cd72](https://github.com/steveyackey/devrig/commit/d86cd728d9576d3982b9c0ce7a707ad5dceddfa9))
* **v0.6:** Claude Code skill + Cluster addons ([6ef1177](https://github.com/steveyackey/devrig/commit/6ef11771293a900a364862518ca3b96569d66a66))
* **v0.7:** Dashboard redesign — Tailwind v4, component library, visual identity ([6c93e4a](https://github.com/steveyackey/devrig/commit/6c93e4a993b4b805d916cfb20bce24a1876d9f42))
* **v0.8:** Dashboard redesign — WCAG AA, metrics charts, telemetry generator ([7681e81](https://github.com/steveyackey/devrig/commit/7681e8114b84f28fa006757b515155ae56985dfe))


### Bug Fixes

* **ci:** create dashboard/dist stub for rust-embed derive ([9460a83](https://github.com/steveyackey/devrig/commit/9460a83331433107deb2b8100981630a0d4f4504))
* constrain chart overflow and add consistent padding across dashboard ([4a21182](https://github.com/steveyackey/devrig/commit/4a211820a3cdf1c8135a8d423685a872e7297e23))
* CSS cascade layer conflict breaking all Tailwind spacing utilities ([b917d99](https://github.com/steveyackey/devrig/commit/b917d99e0fb790171fc270cae29cb291c823c45b))
* Ctrl+C runs stop (preserve state), not delete ([fc86ddc](https://github.com/steveyackey/devrig/commit/fc86ddc1679e360dfc3353a8fe5ce448ba3cb73e))
* **windows:** use *mut c_void for HANDLE type in windows-sys 0.59 ([477f000](https://github.com/steveyackey/devrig/commit/477f000382f53d6f32eae302a74b2026189b1dd5))
* **windows:** use ping for sleep test, ignore flaky watcher test ([b2a9982](https://github.com/steveyackey/devrig/commit/b2a99825391354f7aacbde9ade1b440404cea1eb))


### Performance Improvements

* use embedded server for e2e tests, Vite only for screenshots ([0cfa02e](https://github.com/steveyackey/devrig/commit/0cfa02e1d2ab12aa7a746992d7bc3efc0c488383))

## Changelog
