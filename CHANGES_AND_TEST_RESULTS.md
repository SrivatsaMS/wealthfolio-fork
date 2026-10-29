# Changes and test results

**Repository:** `https://github.com/wealthfolio/wealthfolio`, cloned at
`669418ccea9b15212abb81bc8ce3db0fbd18c646` (branch `main`, v3.11.0).
`afadil/wealthfolio` redirects to the same repository.

**Delivered as:** the complete repository (`wealthfolio-fork-full-repo.zip`)
with the change committed on `main` and `origin` set to
`https://github.com/SrivatsaMS/wealthfolio-fork.git`. Nothing has been pushed:
you upload it (`IMPLEMENTATION_GUIDE.md` §3), and GitHub Actions builds the
executables.

**Environment:** Linux cloud container with Node 22.23 and pnpm 10.33.4.

- The repository pins Rust 1.98.1. The network blocked downloading it, so every
  Rust command here ran on **Rust 1.97.0**.
- Docker, Android SDK/NDK and macOS were not available, so those builds were
  **not run here**. GitHub Actions has to run them.

## 1. Features completed

| Requirement                                                                                                                                                                                                                                                                                                                                          | Status                                                                                            | Where                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Fixed deposits: create, view, edit, delete; simple and compound interest; compounding frequency; day count; issuer amount kept separate from the estimate; tax; status; accrued valuation with no double counting                                                                                                                                    | Done and tested                                                                                   | `crates/core/src/assets/fixed_deposit.rs`, UI in `apps/frontend/src/pages/asset/alternative-assets/products/`          |
| LIC policies: all listed fields; monthly, quarterly, half-yearly, yearly, single and custom frequencies; schedule; next due date; paid, remaining and overdue counts; totals; maturity estimates (documents, assumed return, sum assured + bonus), none shown as guaranteed; premium payments as transactions; duplicate prevention; payment history | Done and tested                                                                                   | `crates/core/src/assets/insurance_policy.rs`, `record-premium-dialog.tsx`, `product-overview.tsx`                      |
| Portfolio integration (holdings, net worth, net-worth history)                                                                                                                                                                                                                                                                                       | Done and tested                                                                                   | `calculated_value.rs`, `alternative_assets_service.rs`, `net_worth_service.rs`                                         |
| UPI payment method, kept separate from category: create/edit, filter, search, period totals by category, CSV export; existing data compatible                                                                                                                                                                                                        | Done and tested (CSV _import_ mapping not added)                                                  | `crates/spending/src/payment_method.rs`, spending search, `payment-method-summary-sheet.tsx`, `cash-activity-form.tsx` |
| Central server as source of truth; Android and macOS connect to it                                                                                                                                                                                                                                                                                   | Done ("server mode"). Server side tested; device navigation **not tested on real devices**        | `apps/tauri/src/server_mode.rs`, `features/server-mode/`, `pages/settings/server/`                                     |
| Server URL setting: test, auth detection, status, last connection time, errors, disconnect                                                                                                                                                                                                                                                           | Done                                                                                              | same                                                                                                                   |
| Concurrent edits                                                                                                                                                                                                                                                                                                                                     | 409 Conflict for fixed deposit and policy edits; other entities stay last-write-wins (documented) | `alternative_repository.rs`                                                                                            |
| Retries and duplicates                                                                                                                                                                                                                                                                                                                               | Idempotency keys (premiums keyed per instalment)                                                  | existing unique index, `premiumPaymentTag`                                                                             |
| Deletions reach other clients                                                                                                                                                                                                                                                                                                                        | Done (server delete + event stream refresh)                                                       | tested in `self_hosted_sync.rs`                                                                                        |
| Docker Compose with a Tailscale sidecar, **no `.env` file**; server at `http://<tailscale-ip>:8088` (optional HTTPS via `tailscale serve`); no published ports; auth enforced                                                                                                                                                                        | Done; **not run here** (no Docker)                                                                | `compose.tailscale.yml`, `deploy/tailscale/serve.json`                                                                 |
| Plain HTTP on the Tailscale IP: apps accept `http://` only for Tailscale IPs (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`), `.ts.net` names and loopback; Android allows cleartext; macOS allows HTTP web content; login cookie not marked Secure over HTTP                                                                                               | Done and tested (server + URL rules); device behaviour not tested                                 | `server_mode.rs`, `gen/android/app/build.gradle.kts`, `Info.plist`, `self_hosted_sync.rs`                              |
| GitHub Actions: Docker (amd64 + arm64, GHCR, SHA tags), Android APK + AAB (signed when secrets exist), macOS .app + .dmg (signed/notarized when secrets exist), release upload                                                                                                                                                                       | Written and passes `actionlint`; **not run**                                                      | `.github/workflows/custom-build.yml`                                                                                   |
| Documentation                                                                                                                                                                                                                                                                                                                                        | Done                                                                                              | `IMPLEMENTATION_GUIDE.md`, `docs/self-host/README.md`                                                                  |

**Architecture choices.** The reasons are in `IMPLEMENTATION_GUIDE.md` §1.

- There are no new SQL migrations and no change to the `assets` CHECK
  constraint.
- Fixed deposits and policies are `OTHER` assets with typed metadata.
- The payment method lives in the transaction's metadata.
- Sync is done by the apps opening the server, not by copying databases or
  replicating records.

**Other behaviour changes, each deliberate:**

- `assets.updated_at` is now refreshed when alternative-asset details are
  edited. This is needed for the conflict check.
- Server: `WF_AUTH_REQUIRED=true` now also enforces authentication on a loopback
  listen address. This is needed for the Tailscale sidecar setup. Behaviour
  without the variable is unchanged.
- Server: editing fixed deposit or policy terms triggers a portfolio
  recalculation, so other clients refresh.
- Desktop/mobile: a new `server-mode` plugin, five commands, and a startup gate
  that only acts when a server URL has been saved.
- Android: `usesCleartextTraffic` is now `true` in release builds; macOS:
  `NSAppTransportSecurity` allows HTTP web content. Both are needed for
  `http://100.x.y.z:8088`. The app code limits plain HTTP to Tailscale
  addresses, `.ts.net` names and loopback.
- Frontend: SHA-256 hashing (CSV import idempotency keys, addon rule ids) falls
  back to a pure implementation when `crypto.subtle` is missing, which is the
  case on plain-HTTP pages.
- `cache-warm.yml`: upstream-only cache jobs skip themselves outside
  `wealthfolio/wealthfolio`, so they don't fail in your repository.

## 2. Files

### New

- `crates/core/src/assets/fixed_deposit.rs`, `fixed_deposit_tests.rs`,
  `insurance_policy.rs`, `insurance_policy_tests.rs`, `calculated_value.rs`
- `crates/spending/src/payment_method.rs`
- `crates/storage-sqlite/tests/deposits_and_policies.rs`
- `apps/server/tests/self_hosted_sync.rs`
- `apps/tauri/src/server_mode.rs`
- `apps/frontend/src/pages/asset/alternative-assets/products/`:
  - `index.ts`, `product-metadata.ts` (+ test)
  - `product-fields.tsx`, `fixed-deposit-form-dialog.tsx`,
    `insurance-policy-form-dialog.tsx`, `record-premium-dialog.tsx`,
    `product-overview.tsx`
  - `product-dialogs.test.tsx`
- `apps/frontend/src/features/spending/types/payment-method.ts`,
  `lib/payment-methods.ts` (+ test),
  `components/payment-method-summary-sheet.tsx`
- `apps/frontend/src/features/server-mode/`: `api.ts`, `shell-return.ts` (+
  test), `server-mode-gate.tsx`
- `apps/frontend/src/pages/settings/server/server-settings-page.tsx`
- `compose.tailscale.yml`, `deploy/tailscale/serve.json`
- `apps/frontend/src/lib/sha256.ts` (+ test)
- `.github/workflows/custom-build.yml`
- `IMPLEMENTATION_GUIDE.md`, `CHANGES_AND_TEST_RESULTS.md`

### Modified

- **Rust:**
  - `crates/core/src/assets/{mod.rs, alternative_assets_model.rs, alternative_assets_service.rs, alternative_assets_traits.rs}`
  - `crates/core/src/portfolio/net_worth/{net_worth_service.rs, net_worth_service_tests.rs}`
  - `crates/spending/src/{lib.rs, cash_activities/model.rs, cash_activities/service.rs}`
  - `crates/storage-sqlite/src/assets/alternative_repository.rs`
  - `crates/storage-sqlite/tests/loan_actions.rs`
  - `apps/server/src/{config.rs, api/alternative_assets.rs, api/spending.rs}`
  - `apps/tauri/{Cargo.toml, src/lib.rs, src/commands/alternative_assets.rs, src/commands/spending.rs}`
  - `Cargo.lock` (one new path dependency edge: `wealthfolio-http` for the app)
- **Frontend:**
  - `App.tsx`, `main.tsx`, `app-screen-routes.tsx`, `lib/types.ts`
  - `adapters/shared/alternative-assets.ts`, `adapters/web/core.ts`
  - `features/spending/{adapters/cash-activities.ts, types/cash-activity.ts, components/cash-activity-form.tsx, components/cash-activity-form.test.tsx, components/spending-transactions-tab.tsx, components/transactions-filter-bar.tsx}`
  - `pages/asset/alternative-asset-content.tsx`,
    `pages/asset/alternative-assets/components/alternative-asset-quick-add-modal.tsx`,
    `pages/settings/settings-layout.tsx`
  - translations: `asset.json`, `settings.json` and `spending.json` in all 11
    locales
  - `addons/spending-rule-key.ts`, `pages/activity/import/utils/idempotency.ts`
    (SHA-256 fallback)
- **App packaging:** `apps/tauri/Info.plist`,
  `apps/tauri/gen/android/app/build.gradle.kts`
- **Other:** `.github/workflows/cache-warm.yml`, `docs/self-host/README.md`

## 3. Commands run and results

### Baseline (unchanged upstream code)

| Command                                | Result      |
| -------------------------------------- | ----------- |
| `cargo test -p wealthfolio-core --lib` | 2107 passed |

### After the changes

| Command                                                                                                                                                                   | Result                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo test -p wealthfolio-core`                                                                                                                                          | **2133 passed** (26 new), plus 12 health property tests passed                                                                                                                                                                                                                                                                                                                                                                         |
| `cargo test -p wealthfolio-spending --lib`                                                                                                                                | **202 passed** (includes 5 new payment-method tests)                                                                                                                                                                                                                                                                                                                                                                                   |
| `cargo test --no-fail-fast -p wealthfolio-spending -p wealthfolio-storage-sqlite -p wealthfolio-server`                                                                   | **972 passed, 2 failed.** The 2 failures are `secrets::tests::failed_write_preserves_existing_vault` and `portable_permissions::read_only_output_preserves_inputs_and_saved_snapshots`. Both check that a read-only directory refuses a write, which cannot happen when tests run as **root**, as in this container (the first one's assertion message says "Run permission tests as an unprivileged user"). Neither file was changed. |
| `cargo test -p wealthfolio-storage-sqlite --test deposits_and_policies`                                                                                                   | 4 passed (real migrated SQLite)                                                                                                                                                                                                                                                                                                                                                                                                        |
| `cargo test -p wealthfolio-storage-sqlite --test loan_actions`                                                                                                            | 17 passed                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `cargo test -p wealthfolio-server --test self_hosted_sync`                                                                                                                | 5 passed: login over plain HTTP on a Tailscale IP (cookie not Secure, authenticated call OK); two clients on one server; FD seen, edited, conflict refused, invalid terms refused, delete propagated; duplicate premium refused; UPI metadata stored; summary endpoint; unauthorized requests get 401                                                                                                                                  |
| `cargo test -p wealthfolio-server --lib config`                                                                                                                           | 15 passed (includes the new loopback `WF_AUTH_REQUIRED=true` case)                                                                                                                                                                                                                                                                                                                                                                     |
| `cargo test -p wealthfolio-app --lib server_mode` (Linux desktop build)                                                                                                   | 5 passed (includes plain HTTP to Tailscale IPs and `.ts.net`; refuses LAN IPs and addresses outside `100.64.0.0/10`)                                                                                                                                                                                                                                                                                                                   |
| `cargo clippy -p wealthfolio-core -p wealthfolio-spending -p wealthfolio-storage-sqlite -p wealthfolio-server -p wealthfolio-app --all-targets -- -D warnings`            | clean                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `cargo fmt --all -- --check`                                                                                                                                              | clean                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `pnpm type-check`                                                                                                                                                         | passed                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `pnpm lint:quiet`                                                                                                                                                         | passed                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `pnpm format:check`                                                                                                                                                       | passed                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `pnpm themes:check`                                                                                                                                                       | passed                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `pnpm --filter frontend i18n:check`                                                                                                                                       | 0 issues                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `vitest`: payment-methods, product-metadata, product-dialogs, server-mode, cash-activity-form, transactions-filter-bar, spending-transactions-tab, adapter-command-parity | all passed                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `pnpm --filter frontend exec vitest run` (full suite)                                                                                                                     | **4707 passed, 17 failed** (506 files). All 17 failures are in `src/addons/iframe/*` (16) and `src/lib/formatting.test.ts` (1). **The same 17 fail on unmodified upstream `669418cc`** in this container, which runs Node 22 instead of the pinned Node 24. One regression this change caused (the quick-add modal test mocks `@wealthfolio/ui`) was found and fixed.                                                                  |
| `vitest` after the HTTP/Tailscale update: sha256, server-mode, spending, alternative-assets, import, adapter parity                                                       | **919 passed** (102 files)                                                                                                                                                                                                                                                                                                                                                                                                             |
| `actionlint .github/workflows/custom-build.yml`                                                                                                                           | no findings                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `pnpm build` (web bundle)                                                                                                                                                 | passed. A first attempt failed on a `Select` callback type that `type-check` had not flagged; it was fixed and the build was rerun.                                                                                                                                                                                                                                                                                                    |
| `pnpm build:tauri` (app bundle, frontend only)                                                                                                                            | passed                                                                                                                                                                                                                                                                                                                                                                                                                                 |

### Not run here

| Command                                                                     | Reason                               |
| --------------------------------------------------------------------------- | ------------------------------------ |
| Docker image build; `docker compose … up`                                   | No Docker daemon in this environment |
| `pnpm tauri android build`                                                  | No Android SDK/NDK                   |
| `pnpm tauri build` for macOS                                                | Needs a macOS runner                 |
| Clippy/tests on Rust 1.98.1                                                 | Toolchain download blocked           |
| End-to-end on real devices (app window → `http://100.x:8088` server → back) | Requires your devices and tailnet    |

## 4. Known issues and limitations

- **Not tested on devices:**
  - Server-mode navigation on Android and macOS: opening the `http://100.x:8088`
    or HTTPS address in the app window, and "Disconnect this app" returning to
    the local app via `?wf_disconnect=1`.
  - Run the checks in `IMPLEMENTATION_GUIDE.md` §15.
- **Offline editing** is not available in server mode.
- **Conflict detection** applies to fixed deposit and policy edits only; other
  entities keep last-write-wins.
- **CSV import** has no payment-method column mapping.
- **Translations:** new fixed deposit, policy and server-mode text is English in
  all locales; the payment-method labels are translated.
- **Fixed deposit maths** does not model bank short-tenor or business-day rules.
- **The macOS workflow** overrides `signingIdentity` with an ad-hoc signature
  (`-`) when no Apple secrets are set, and disables updater artifacts. Confirm
  on the first CI run.

## 5. What you do

1. Upload the repository to `SrivatsaMS/wealthfolio-fork` with GitHub Desktop or
   `git push` (`IMPLEMENTATION_GUIDE.md` §3). Don't use the website's upload
   button.
2. Set **Settings → Actions → General → Workflow permissions** to read and write
   (§3.4).
3. **Custom build** runs on the push; download the APK, `.dmg` and the Docker
   image address from the run (§4).
4. Add the `wealthfolio` service to your Tailscale-sidecar stack, with the
   secrets inline (§6).
5. Move existing data with a portable backup (§13), then connect the apps to
   `http://<tailscale-ip>:8088` (§8).
