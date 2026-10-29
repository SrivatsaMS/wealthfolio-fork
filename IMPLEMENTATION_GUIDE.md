# Implementation guide: fixed deposits, LIC policies, UPI, self-hosted sync

This repository is upstream Wealthfolio 3.11.0
(`https://github.com/wealthfolio/wealthfolio`, commit
`669418ccea9b15212abb81bc8ce3db0fbd18c646`) plus one commit that adds:

- fixed deposits
- LIC policies
- UPI expense tracking
- **server mode**, which lets the Android and macOS apps use your self-hosted
  server
- a GitHub Actions workflow that builds everything on GitHub

**Nothing has to be built on your computer.** You upload the repository, and
GitHub Actions produces:

- the Android APK (and AAB)
- the macOS `.app` and `.dmg`
- the Docker image for your server

Repositories:

| Repository                                       | Use                                                |
| ------------------------------------------------ | -------------------------------------------------- |
| `https://github.com/SrivatsaMS/wealthfolio-fork` | Test repository for this edited code (steps below) |
| `https://github.com/SrivatsaMS/wealthfolio`      | Your fork of upstream; optional, section 3.3       |

Values you must replace are written in CAPITALS, for example
`REPLACE_WITH_TAILSCALE_IP`.

Who does what:

| Done already, in this repository                               | Done by you on GitHub                                                                  | Done by you on the server and devices                                                        |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| All code, tests, workflow and docs; one local commit on `main` | Upload (push), run **Custom build**, download artifacts, optional secrets and releases | Docker Compose with your Tailscale sidecar, installing the apps, entering the server address |

---

## 1. Quick start

1. Upload the repository to `SrivatsaMS/wealthfolio-fork` (section 3).
2. GitHub runs **Custom build** automatically on the push to `main`.
3. Wait about 30–60 minutes, then download the artifacts (section 4).
4. Start the server with your Tailscale sidecar (section 6). Check
   `http://REPLACE_WITH_TAILSCALE_IP:8088` in a browser.
5. Install the APK and the macOS app. In **Settings → Self-hosted server**,
   enter `http://REPLACE_WITH_TAILSCALE_IP:8088` (section 8).

---

## 2. What was built, and how it fits Wealthfolio

Wealthfolio is a React frontend (`apps/frontend`) that runs in two runtimes:

| Runtime                          | Code                        | Data                         |
| -------------------------------- | --------------------------- | ---------------------------- |
| Desktop and mobile app (Tauri 2) | `apps/tauri`                | Local SQLite on the device   |
| Self-hosted web server (Axum)    | `apps/server`, Docker image | SQLite in the `/data` volume |

Both runtimes call the same Rust business logic in `crates/core`,
`crates/spending` and `crates/storage-sqlite`.

### 2.1 Fixed deposits and LIC policies

- They are alternative assets, built the same way as loans. The terms are stored
  in the asset's metadata (`kind = OTHER`, with `sub_type` set to
  `fixed_deposit` or `insurance_policy`).
- **No database schema change was needed.** Giving them their own `kind` would
  have meant rebuilding the `assets` table, because SQLite cannot change the
  existing `CHECK (kind IN …)` constraint. That rebuild is risky for existing
  data.
- The calculations are pure Rust and live in one place:
  - `crates/core/src/assets/fixed_deposit.rs`
  - `crates/core/src/assets/insurance_policy.rs`
  - `crates/core/src/assets/calculated_value.rs`

  Holdings, current net worth and net-worth history all use them. There is no
  second calculation engine in TypeScript.

- LIC premium payments are ordinary `WITHDRAWAL` activities. Each one is tagged
  with `metadata.policy_premium = {policy_id, installment}` and carries the
  idempotency key `policy-premium:<policy>:<instalment>`. The existing unique
  index `ux_activities_idempotency_key` refuses a second copy, so a retry or a
  second device cannot record the same instalment twice.

### 2.2 UPI

- The payment method is stored on the transaction as `metadata.payment_method`.
  The allowed values are `upi`, `cash`, `debit_card`, `credit_card`,
  `bank_transfer` and `other`.
- The category stays separate. A restaurant bill is category `Food` with payment
  method `upi`.
- The existing activity create and update paths already accept metadata, and an
  update merges the keys it sends. Exports, backups and device sync already
  carry metadata.
- **No migration is needed.** Older transactions simply show "Not specified".

### 2.3 Central synchronization ("server mode")

The app is local-first. Its existing device sync goes through Wealthfolio
Connect, a cloud service. Copying a SQLite database between devices is unsafe.

The smallest reliable design is this: your self-hosted server is the only copy
of the data, and the desktop and Android apps open that server in place of their
local database.

```text
 Android app ─┐   http://100.x.y.z:8088 (WireGuard-encrypted)   ┌─ Docker: tailscale sidecar ─────┐
 (server mode)│   or https://wealthfolio.<tailnet>.ts.net        │  tailnet IP 100.x.y.z            │
 macOS app ───┼─────────────────────────────────────────────────►│  wealthfolio :8088 (shared netns)│
 (server mode)│   login: password / OIDC                         │  SQLite in the /data volume      │
 Browser ─────┘   live refresh: /api/v1/events/stream            └──────────────────────────────────┘
```

- Every read and write goes to the server's authenticated API (`/api/v1/...`).
  There is no copy of the data on the client to drift out of sync.
- When one client saves, the server broadcasts a domain event on
  `/api/v1/events/stream` (server-sent events). The other open clients then
  refresh.
- Deletions, retries and duplicates:
  - A deletion is a server delete, so every client sees it immediately.
  - New activities, including expenses and premiums, get an idempotency key, so
    a repeated request is refused with "Duplicate activity detected".
- Conflicts:
  - Fixed deposit and policy edits send the `updatedAt` value the client loaded.
    If another device saved in the meantime, the server answers **409 Conflict**
    and nothing is overwritten.
  - Other edits (expenses, accounts, categories) use the existing behaviour:
    each save is one atomic server write, and the most recent save wins. See
    "Known limitations".
- **Offline editing is not supported in server mode.** If the server cannot be
  reached at startup, the app says so. It offers "Try again" or "Use this
  device's local data", and it never merges the two.

---

## 3. Upload the repository to GitHub

The ZIP contains the **complete repository**:

- every source file
- the workflows in `.github/`
- a Git repository with two commits on branch `main`:
  1. _Wealthfolio 3.11.0_, an unmodified snapshot of upstream commit `669418cc`
  2. _Add fixed deposits, LIC policies, UPI tracking and self-hosted server
     mode_, this change

  On GitHub you can open the second commit to see every changed line. Upstream's
  5,000-commit history is left out; it would add about 200 MB and isn't needed
  to build.

`origin` already points at `https://github.com/SrivatsaMS/wealthfolio-fork.git`.

> **Do not use the GitHub website's "Upload files" button.**
>
> - It accepts at most 100 files per upload. This repository has about 7,800.
> - It does not reliably upload hidden folders, so `.github/workflows` would be
>   lost and Actions would never run.
>
> Use **GitHub Desktop** (no command line) or **git**, as below. GitHub only
> stores what you upload; it does not unpack a ZIP into a repository.

### 3.1 With GitHub Desktop (no terminal)

1. Unzip `wealthfolio-fork-full-repo.zip`. You get a folder `wealthfolio-fork`.
2. In GitHub Desktop, choose **File → Add local repository…**, select that
   folder, then **Add repository**.
3. Check that the current branch is `main`. Then click **Push origin**, or
   **Publish branch** if it is offered. Sign in to GitHub if asked.
4. If GitHub Desktop says the remote has commits you don't have, your
   `wealthfolio-fork` repository was created with a README or licence. Two ways
   to fix it:
   - Delete that test repository on GitHub, recreate it **empty** (no README, no
     licence, no .gitignore), and push again.
   - Or use the terminal command in 3.2 step 4.

### 3.2 With git (terminal)

```bash
unzip wealthfolio-fork-full-repo.zip
cd wealthfolio-fork
git log --oneline -2          # the change commit on top of upstream 669418cc
git remote -v                 # origin → https://github.com/SrivatsaMS/wealthfolio-fork.git
git push -u origin main
```

Sign in when Git asks. Use a browser login, the GitHub CLI (`gh auth login`), or
an SSH key. Never put a token in this repository.

Step 4, only if `git push` is rejected because the new, otherwise-empty
`wealthfolio-fork` repository already had an initial README commit:

```bash
git push -u origin main --force   # overwrites only that auto-created README commit
```

Use `--force` only for this brand-new test repository. It replaces whatever is
on GitHub's `main` branch there.

### 3.3 Later: put the same change on your real fork (optional)

Your real fork has upstream's full history, and this ZIP has a compact one. So
copy the change commit across instead of pushing `main`:

```bash
git clone https://github.com/SrivatsaMS/wealthfolio.git
cd wealthfolio
git switch -c custom/fd-lic-upi-selfhost 669418ccea9b15212abb81bc8ce3db0fbd18c646
git fetch /path/to/wealthfolio-fork main
git cherry-pick FETCH_HEAD          # applies exactly the change commit
git push -u origin custom/fd-lic-upi-selfhost
```

Then open a pull request from `custom/fd-lic-upi-selfhost` into `main` **in your
fork**.

- If your fork's `main` is newer than `669418cc`, run `git rebase main` after
  the cherry-pick.
- If that rebase stops on a conflict, resolve the marked files, then run
  `git add <file>` and `git rebase --continue`.
- No force push is needed.

### 3.4 After uploading: workflow permissions

New repositories run Actions by default. Do this once:

1. Open **Settings → Actions → General**.
2. Under **Workflow permissions**, choose **Read and write permissions**, then
   **Save**.

The Docker job needs this to publish the image to GitHub Container Registry, and
the release step needs it to attach files.

---

## 4. Build everything with GitHub Actions

### 4.1 What runs

**File:** `.github/workflows/custom-build.yml`

**Runs automatically on:**

- every push to `main` or to `custom/**` branches
- pull requests
- **published releases**

You can also start it by hand. All jobs run in parallel on GitHub-hosted
machines, and none needs your computer.

| Job                   | GitHub runner                 | What you get                                                                                                                                                         |
| --------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Lint and tests**    | Ubuntu                        | Translation, format, lint and type checks; all frontend tests; Rust tests. Report only: the build jobs do not wait for it.                                           |
| **Docker image**      | Ubuntu (cross-compiles arm64) | `ghcr.io/srivatsams/wealthfolio-fork` for `linux/amd64` + `linux/arm64`. Tags: full commit SHA, branch name, `latest` (on `main` and releases), and the release tag. |
| **Android APK / AAB** | Ubuntu + Android SDK/NDK      | Artifact `wealthfolio-custom-android-<sha>`: `wealthfolio-custom-android-arm64*.apk` and the `.aab`                                                                  |
| **macOS app**         | macOS 14, twice               | `wealthfolio-custom-macos-aarch64-apple-darwin-<sha>` (Apple Silicon) and `…-x86_64-apple-darwin-<sha>` (Intel), each with `.dmg` + `.app.zip`                       |

The workflow builds the apps with in-app updates switched off. Otherwise the
upstream updater could replace your custom build with an official release.

Other workflows that came with upstream:

- `release.yml` and `docker-publish.yml` only run on tags starting with `v`.
  **Use release tags like `custom-3.11.0-1`** so they never start.
- `cache-warm.yml` now skips itself outside `wealthfolio/wealthfolio`.
- `pr-check.yml` and the other checks run only on pull requests and do no harm.

### 4.2 Run it and download the files (GitHub website)

1. Open `https://github.com/SrivatsaMS/wealthfolio-fork/actions`.
2. Click **Custom build** in the left list.
   - The push already started a run.
   - To start one by hand: **Run workflow**, choose `main`, tick Android, macOS
     and/or Docker, then **Run workflow**.
3. Open the run. Each job shows ✅ when done or ❌ on failure. For a failure,
   click the job, then the red step, and read the end of its log.
4. When the jobs are green, scroll to **Artifacts** at the bottom of the run
   page and download the ZIPs. Artifacts are kept for 90 days.
5. Docker image: open your GitHub profile → **Packages** → **wealthfolio-fork**.
   The pull address is `ghcr.io/srivatsams/wealthfolio-fork:latest`.
   - The package starts **private**.
   - Either make it public (**Package settings → Change visibility**), or log
     the server in once:

     ```bash
     docker login ghcr.io -u SrivatsaMS
     ```

     Use a classic token with only `read:packages` as the password.

### 4.3 Install on Android

1. Unzip the Android artifact.
2. Copy the `.apk` to the phone and open it. Allow "Install unknown apps" for
   your file manager or browser.
3. **Without signing secrets** (the default):
   - The APK is signed with a throwaway test key from that run.
   - It installs, but a later run's APK cannot update it: uninstall first.
     Uninstalling deletes only the app's _local_ data, not the server's.
   - To get updatable APKs, add the Android secrets (section 5.1).
4. The build is arm64, which covers practically all current phones.

### 4.4 Install on macOS

1. Unzip the macOS artifact for your Mac and open the `.dmg`. Drag
   **Wealthfolio** to Applications.
2. **Without Apple secrets** the app is ad-hoc signed and **not notarized**. On
   first launch macOS blocks it. Either:
   - go to **System Settings → Privacy & Security → Open Anyway**, or
   - run `xattr -dr com.apple.quarantine /Applications/Wealthfolio.app`.
3. It uses the same app identifier as the official Wealthfolio app, so it
   replaces that app and keeps its local data.

### 4.5 Releases (optional)

1. Go to **Releases → Draft a new release**.
2. Create a tag `custom-3.11.0-1` on `main`. Do not start the tag with `v`.
3. Click **Publish release**.
4. **Custom build** runs again. It attaches the APK, AAB, `.dmg` and `.app.zip`
   to the release, and tags the Docker image `custom-3.11.0-1` and `latest`.

---

## 5. Signed builds (optional)

Development builds need no secrets. Add these only if you want:

- Android APKs that update in place, or
- a Developer ID signed and notarized macOS app.

Add them under **Settings → Secrets and variables → Actions → New repository
secret**. Never put them in the repository.

### 5.1 Secrets (Settings → Secrets and variables → Actions → New repository secret)

Never commit any of these to the repository.

**Android release signing.** Create the key once, on your computer:

```bash
keytool -genkeypair -v -keystore wealthfolio-release.jks -alias wealthfolio -keyalg RSA -keysize 4096 -validity 10000
base64 -i wealthfolio-release.jks | pbcopy        # macOS (Linux: base64 -w0 wealthfolio-release.jks)
```

| Secret                      | Value             |
| --------------------------- | ----------------- |
| `ANDROID_KEYSTORE_BASE64`   | the base64 text   |
| `ANDROID_KEYSTORE_PASSWORD` | keystore password |
| `ANDROID_KEY_ALIAS`         | `wealthfolio`     |
| `ANDROID_KEY_PASSWORD`      | key password      |

Keep the `.jks` file and its passwords backed up. If you lose them, installed
apps cannot be updated.

**macOS signing and notarization.** These need a paid Apple Developer account.

| Secret                       | Value                                                     |
| ---------------------------- | --------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | base64 of your exported "Developer ID Application" `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password                                       |
| `APPLE_SIGNING_IDENTITY`     | e.g. `Developer ID Application: Your Name (TEAMID)`       |
| `APPLE_ID`                   | your Apple ID e-mail                                      |
| `APPLE_PASSWORD`             | an app-specific password from appleid.apple.com           |
| `APPLE_TEAM_ID`              | your team ID                                              |

Without these, the build is ad-hoc signed and the log says so. It is never
described as signed or notarized.

**Docker:** no secret is needed. The workflow pushes to GHCR with
`GITHUB_TOKEN`. Under **Settings → Actions → General → Workflow permissions**,
keep the default, or allow "Read and write" if your organisation restricts
package writes.

---

## 6. Run the server with your Tailscale sidecar

No `.env` file is needed. `compose.tailscale.yml` is a complete example: a
Tailscale sidecar plus Wealthfolio sharing its network, with every setting
written inline.

| Setting                 | Value                                                                                                                                                                                   |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WF_LISTEN_ADDR`        | `0.0.0.0:8088` inside the sidecar's network namespace. The server answers at **`http://<tailscale-ip>:8088`**. No host port is published, so your LAN and the internet cannot reach it. |
| Authentication          | Required. The server refuses to start on a non-loopback address without `WF_AUTH_PASSWORD_HASH` or OIDC. Being on the tailnet is not enough to see your data.                           |
| `WF_COOKIE_SECURE`      | `auto`. The login cookie works over `http://100.x:8088` and is marked Secure over `https://…ts.net`. Do **not** set it to `true` if you use the HTTP address.                           |
| `WF_CORS_ALLOW_ORIGINS` | Must list exactly the addresses you open: `http://REPLACE_WITH_TAILSCALE_IP:8088`, plus `https://wealthfolio.REPLACE_WITH_TAILNET.ts.net` if you use HTTPS.                             |
| Database                | Only in the `wealthfolio-data` volume (`/data/wealthfolio.db`). It is never exposed on a port.                                                                                          |

### 6.1 With your existing sidecar

Copy the `wealthfolio` service from `compose.tailscale.yml` into your own stack:

```yaml
wealthfolio:
  image: ghcr.io/srivatsams/wealthfolio-fork:latest
  restart: unless-stopped
  network_mode: service:REPLACE_WITH_YOUR_SIDECAR_SERVICE_NAME
  volumes:
    - wealthfolio-data:/data
  environment:
    WF_LISTEN_ADDR: "0.0.0.0:8088"
    WF_DB_PATH: /data/wealthfolio.db
    WF_SECRET_KEY: "REPLACE_WITH_OUTPUT_OF_openssl_rand_base64_32"
    WF_AUTH_PASSWORD_HASH: "REPLACE_WITH_DOUBLED_DOLLAR_ARGON2_HASH"
    WF_CORS_ALLOW_ORIGINS: "http://REPLACE_WITH_TAILSCALE_IP:8088"
    WF_COOKIE_SECURE: "auto"
```

`REPLACE_WITH_TAILSCALE_IP` is your sidecar's tailnet IP. Find it in the
Tailscale admin console, or with:

```bash
docker exec REPLACE_WITH_SIDECAR_CONTAINER tailscale ip -4
```

### 6.2 Generate the two secrets (once, on any machine with Docker)

```bash
# WF_SECRET_KEY: keep a copy, backups and the credential vault need it
docker run --rm alpine sh -c 'head -c 32 /dev/urandom | base64'

# WF_AUTH_PASSWORD_HASH: replace YOUR-PASSWORD; the sed doubles each $ for YAML
docker run --rm alpine sh -c "apk add -q argon2 && printf 'YOUR-PASSWORD' | argon2 \"\$(head -c 8 /dev/urandom | od -An -tx1 | tr -d ' \n')\" -id -e" | sed 's/\$/$$/g'
```

In a Compose YAML file every `$` in the hash must be written as `$$`. The `sed`
does that for you.

### 6.3 Start and check

```bash
docker compose -f compose.tailscale.yml up -d     # or: docker compose up -d in your own stack
docker compose -f compose.tailscale.yml logs -f wealthfolio
```

From any device on your tailnet:

```bash
curl -fsS http://REPLACE_WITH_TAILSCALE_IP:8088/api/v1/healthz && echo         # 200 OK
curl -fsS http://REPLACE_WITH_TAILSCALE_IP:8088/api/v1/auth/status && echo     # {"requiresPassword":true,...}
curl -s -o /dev/null -w '%{http_code}\n' http://REPLACE_WITH_TAILSCALE_IP:8088/api/v1/accounts   # 401
```

Then open `http://REPLACE_WITH_TAILSCALE_IP:8088` in a browser and sign in.

**Optional HTTPS.** If MagicDNS and HTTPS certificates are on in your tailnet,
the bundled `deploy/tailscale/serve.json` also serves
`https://wealthfolio.<tailnet>.ts.net`. Add that address to
`WF_CORS_ALLOW_ORIGINS`. With your own sidecar, point its serve config at
`http://127.0.0.1:8088`.

### 6.4 What plain HTTP over Tailscale means

- **Traffic:** between your devices it is encrypted end to end by Tailscale
  (WireGuard). Browsers still label the page "Not secure", because they cannot
  see that layer.
- **Who can connect:** only devices on your tailnet. The app's login is still
  required.
- **Browser features** that require HTTPS are unavailable on the HTTP address:
  - Copy-to-clipboard buttons may do nothing.
  - Hashing during activity CSV import is handled: a built-in SHA-256 fallback
    is used where `crypto.subtle` is missing.

  Use the `https://…ts.net` address if you need everything.

---

## 7. Updating the server

```bash
docker compose -f compose.tailscale.yml pull wealthfolio
docker compose -f compose.tailscale.yml up -d wealthfolio
```

Each push to `main` publishes a new `latest`. Pin a specific build with
`ghcr.io/srivatsams/wealthfolio-fork:sha-<full-commit-sha>`, or with a release
tag. Before pulling an update that crosses upstream versions, take a backup
(section 14). The server also saves a snapshot automatically before running
database migrations.

---

## 8. Connect the Android and macOS apps

The steps are the same on both apps.

1. Install the build (sections 4.3 and 4.4). Keep the **Tailscale app** running
   and signed in on the device.
2. In Wealthfolio, open **Settings → Self-hosted server**.
3. Enter `http://REPLACE_WITH_TAILSCALE_IP:8088`. Typing just
   `REPLACE_WITH_TAILSCALE_IP:8088` also works: a bare Tailscale IP defaults to
   `http://`.
4. Choose **Test connection**. You should see "Server found…".
5. Read **Data already on this device**, tick the box, then choose **Connect and
   open**.
6. Sign in with your server password.

**Which addresses the app accepts:**

| Address                                                               | Accepted?                                     |
| --------------------------------------------------------------------- | --------------------------------------------- |
| `https://…`                                                           | Yes, anywhere                                 |
| `http://` + a Tailscale IP (`100.64.0.0/10` or `fd7a:115c:a1e0::/48`) | Yes                                           |
| `http://` + a `.ts.net` name                                          | Yes                                           |
| `http://127.0.0.1`                                                    | Yes (for testing)                             |
| Any other plain-HTTP address, e.g. `http://192.168.1.10`              | No, because it would expose your login cookie |

TLS certificates are always verified for `https://`.

To support this, the Android build allows cleartext traffic and the macOS build
allows HTTP web content. The app itself only ever opens the addresses in the
table above.

**What is stored on the device:**

- only the server address
- the login session, as an HttpOnly cookie in the system web view

The password is never stored.

**Using it:**

- **Startup:** if the server answers, the app opens it. If not, you see "Cannot
  reach your server", with **Try again** or **Use this device's local data**.
- **Leaving server mode:** in server mode, open **Settings → Self-hosted server
  → Disconnect this app**. Nothing on the server is deleted.

---

## 9. Database migrations

**No new SQL migration was added**, and none is needed:

| Feature              | Storage                                                                                    | Existing rows           |
| -------------------- | ------------------------------------------------------------------------------------------ | ----------------------- |
| Fixed deposit        | `assets.metadata` keys `fd_*` on `kind = 'OTHER'`                                          | Untouched               |
| LIC policy           | `assets.metadata` keys `policy_*` on `kind = 'OTHER'`                                      | Untouched               |
| Premium payment      | `activities.metadata.policy_premium`, `activities.idempotency_key` (existing unique index) | Untouched               |
| UPI / payment method | `activities.metadata.payment_method`                                                       | Read as "Not specified" |
| Conflict check       | existing `assets.updated_at`, now refreshed on every detail edit                           | Untouched               |

Upgrading an existing installation still runs any upstream migrations shipped
between your current version and 3.11.0. Wealthfolio saves a verified **Before
database upgrade** snapshot before running pending migrations (see
`docs/self-host/backups.md`). Take your own backup first anyway (section 14).

---

## 10. Fixed deposits

### 10.1 Create one

1. Open **Holdings** (or the Dashboard) and choose **Add asset**.
2. Choose **Fixed deposit**.
3. Fill in the form:
   - name
   - bank or issuer
   - principal
   - currency
   - annual rate (%)
   - interest calculation: compound or simple
   - compounding frequency (monthly, quarterly, half-yearly, yearly)
   - day count
   - interest payout: cumulative or periodic
   - start date and maturity date
   - optionally: the bank's maturity amount, the tax rate on interest, the
     status and notes
4. Click **Save**.

Validation, done in the app and again on the server:

- principal greater than 0
- rate between 0 and 100
- maturity date after the start date
- the bank's maturity amount not below the principal
- tax rate between 0 and 100

### 10.2 Calculations (`crates/core/src/assets/fixed_deposit.rs`)

- Simple interest: `maturity = P × (1 + r × t)`
- Compound interest: `maturity = P × (1 + r/n)^(n × t)`, with n = 12, 4, 2 or 1
- `t` is in years and depends on the day count:

  | Day count                                    | How `t` is worked out                          |
  | -------------------------------------------- | ---------------------------------------------- |
  | Actual/365 (default; usual for Indian banks) | actual days ÷ 365                              |
  | Actual/360                                   | actual days ÷ 360                              |
  | Actual/Actual                                | each calendar year's days ÷ that year's length |
  | 30/360                                       | every month counts as 30 days                  |

- Amounts are rounded to 2 decimals, half away from zero.
- The **expected maturity amount** (calculated) and the **maturity amount from
  the bank** (entered) are shown separately.

**Portfolio value on a given date (no double counting):**

| When                                   | Value                                                           |
| -------------------------------------- | --------------------------------------------------------------- |
| Before the start date                  | None                                                            |
| Cumulative deposit, while running      | Principal + interest accrued so far                             |
| Periodic-payout deposit, while running | Principal (the interest is paid to your bank account)           |
| From maturity                          | The maturity amount (the bank's figure if you entered one)      |
| After you set status **Closed**        | 0, from the closing date (or from maturity if no date is given) |

If you entered the bank's amount, the accrued interest is scaled so the value
reaches that amount exactly on the maturity date.

**Limitation:** some banks apply rules that are not modelled. Examples are
simple interest on tenors under six months, business-day shifts and
bank-specific rounding. For an exact figure, enter the bank's maturity amount.

### 10.3 The detail page

The detail page shows:

- current value
- principal
- expected maturity amount and interest
- the bank's figure and the interest it implies
- interest accrued so far
- estimated tax and the interest after tax
- rate and method
- issuer
- start and maturity dates
- remaining time
- status

**Edit terms** saves only if nobody else changed the deposit since you opened
it.

---

## 11. LIC / insurance policies

### 11.1 Create one

1. Choose **Add asset**, then **LIC / insurance policy**.
2. Fill in the policy details:
   - policy name, number, insurer and type
   - premium amount and currency
   - frequency: monthly, quarterly, half-yearly, yearly, single premium, or
     custom (every N months)
   - commencement date and first premium date
   - policy term and premium paying term (years)
   - maturity date (optional; by default commencement + policy term)
   - **Premiums already paid**: those paid before you started recording them
     here
   - sum assured and declared bonuses
   - the expected maturity benefit from your documents
   - an assumed return (%), only if you want an illustrative estimate
   - status
   - valuation: premiums paid, or manual
3. Click **Save**.

### 11.2 Record a premium

1. On the policy page, choose **Record premium**.
2. Pick the paying account, the date and the amount (the premium is prefilled).
3. Check the instalment number (the next unpaid one is prefilled).
4. Optionally choose a payment method, for example **UPI**.
5. Confirm.

This creates a withdrawal from the paying account. Recording the same instalment
again, from any device, is refused with "This instalment is already recorded."
To remove a payment, use the bin icon in **Payment history**.

### 11.3 Calculations (`crates/core/src/assets/insurance_policy.rs`)

**The premium schedule:**

- Due dates are the first premium date plus k × the frequency in months.
- Each date is worked out from the first one, so month-end dates do not drift.
- The number of premiums is the premium term in months ÷ the frequency (1 for a
  single premium).

**What is counted:**

- **Premiums paid** = premiums already paid before tracking + recorded payments,
  with one payment per instalment (duplicates do not count).
- **Next due** is the first unpaid due date.
- **Overdue** counts the unpaid premiums due on or before today.
- Totals are scheduled amount, paid amount and outstanding amount.

**Maturity figures.** None of them is presented as a guaranteed LIC benefit.

| Figure                         | Where it comes from                                                                                                                                                                        |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| From policy documents          | The amount you entered                                                                                                                                                                     |
| Calculated estimate            | Each scheduled premium grown at your assumed return to maturity (annual compounding, actual/365). Shown only when an assumed return and a maturity date exist, and labelled with the rate. |
| Sum assured + declared bonuses | A reference figure only; it excludes future bonuses                                                                                                                                        |

**Portfolio value:**

- **Premiums paid** (the default): the premiums paid up to each date. The cash
  leaves your bank account and the policy grows by the same amount, so nothing
  is counted twice.
- **Manual**: the latest valuation you enter, for example the surrender value,
  using the asset's existing **Update valuation**.
- **Matured** or **Surrendered** status: the value is 0 from the closing date.
  The payout arrives as a deposit in your bank account.

---

## 12. UPI expense tracking

- **Record a payment method:** in **Spending → Transactions → Add** (or when
  editing a transaction), choose **Payment method → UPI**. The category stays a
  separate field.
- **Filter:** the **Payment method** filter in the transactions toolbar. **Not
  specified** finds older transactions.
- **Search:** typing `upi` in the search box also finds UPI-paid transactions.
- **Totals for a period:** the **Payment methods** button opens a summary for
  the selected date range (this month if no dates are filtered). It shows the
  spent amount, the number of transactions and a breakdown by category for each
  method.
- **Export:** each method in that summary has **Export CSV**. The columns are
  date, account, payee, category, payment method, amount and currency.
- Full activity exports (**Settings → Backup & Export**) and backups include
  `payment_method` inside each transaction's metadata.
- **Import:** backups and portable exports bring the payment method back. CSV
  activity import does not map a payment-method column (see "Known
  limitations").

---

## 13. First connection and moving existing data

The app **never merges** local and server data automatically. That avoids
duplicate assets and transactions. Choose one source of truth:

**A. Your existing data is on a desktop app (most common).**

1. Back it up. On the desktop app (local mode), open **Settings → Backup &
   Export** and export a password-protected portable backup (`.wfbackup`). Store
   it and its password safely.
2. On a fresh server (browser, signed in), open **Settings → Backup & Export →
   restore/import**, choose the `.wfbackup` file and enter its password. Check
   accounts, holdings and transactions.
3. Connect each app to the server (section 8). Their local data stays untouched
   on the device, as a fallback.

**B. Your data is already on the server.** Just connect the apps (section 8).

**C. Data on several devices.** Pick the most complete device as the source, as
in A. Re-enter or re-import the few extra records from the others. Do not
restore several backups into one server: a restore replaces the server database;
it does not merge.

---

## 14. Backup and restore

### 14.1 Server

Find the volume name, then make a consistent copy with the app stopped:

```bash
docker volume ls | grep wealthfolio-data
docker stop wealthfolio
docker run --rm -v REPLACE_WITH_VOLUME_NAME:/data -v "$PWD":/backup alpine \
  tar czf /backup/wealthfolio-data-$(date +%F).tar.gz -C /data .
docker start wealthfolio
```

To restore:

```bash
docker stop wealthfolio
docker run --rm -v REPLACE_WITH_VOLUME_NAME:/data -v "$PWD":/backup alpine \
  sh -c 'rm -rf /data/* && tar xzf /backup/wealthfolio-data-YYYY-MM-DD.tar.gz -C /data'
docker start wealthfolio
```

Keep `WF_SECRET_KEY` somewhere safe. Another option needs no key at all: in the
browser, open **Settings → Backup & Export** and download a password-protected
portable backup (`.wfbackup`).

### 14.2 Devices

In server mode a device holds no financial data, only the server address and a
login cookie. Data from the device's local mode stays on the device. It can be
exported from **Settings → Backup & Export** after disconnecting.

---

## 15. Synchronization checks (once, after setup)

1. **Android → Mac:** on Android, add an expense with payment method UPI. The
   open Mac app shows it within seconds through the live event stream; if not,
   reload.
2. **Mac → Android:** on the Mac, add a fixed deposit. It appears on Android
   under Holdings.
3. **Edit and delete:** change the deposit's rate on Android, then delete it on
   the Mac. Both changes appear on the other device.
4. **Concurrent edit:** open **Edit terms** for the same deposit on both
   devices. Save on one, then the other. The second save is refused with
   "changed on another device".
5. **Duplicate premium:** record instalment N on both devices. The second is
   refused with "already recorded".
6. **Network interruption:** switch Tailscale off on the phone and reopen the
   app. You get "Cannot reach your server". Switch it on and choose **Try
   again**.

---

## 16. Troubleshooting

| Symptom                                                         | Fix                                                                                                                                                                                    |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Actions tab shows nothing after upload                          | The `.github` folder was not uploaded. Use GitHub Desktop or git (section 3), not the web uploader.                                                                                    |
| Docker job: `denied: … permission` / `installation not allowed` | Section 3.4: workflow permissions **Read and write**.                                                                                                                                  |
| Android or macOS job red                                        | Open the job, click the failing step and read the last lines. Re-run with **Re-run failed jobs**: the first run fills caches and can hit timeouts.                                     |
| "Lint and tests" red but builds green                           | The executables are still valid. The report says which check failed.                                                                                                                   |
| Server exits with "Refusing to start…"                          | Set `WF_AUTH_PASSWORD_HASH`, with every `$` doubled in YAML.                                                                                                                           |
| App: "Use https://, or http:// with your Tailscale IP"          | The address is not a Tailscale IP or `.ts.net` name. Use the 100.x address.                                                                                                            |
| App: "Could not connect…"                                       | Tailscale is off on the device, the container is stopped, or the IP/port is wrong. Check with `curl` (section 6.3).                                                                    |
| Login works in the browser but loops in the app                 | The address in `WF_CORS_ALLOW_ORIGINS` must match exactly what you typed, e.g. `http://100.101.102.103:8088` with no trailing slash. Also don't set `WF_COOKIE_SECURE=true` with HTTP. |
| Other devices don't refresh live                                | Reload once. Through a proxy, make sure `/api/v1/events/stream` is not buffered.                                                                                                       |
| "This asset was changed on another device"                      | Expected conflict protection: reload and edit again.                                                                                                                                   |
| Android "App not installed"                                     | APK from a different test key: uninstall the old one, or set up signing (section 5).                                                                                                   |
| macOS "damaged / can't be opened"                               | Unsigned build: section 4.4 step 2.                                                                                                                                                    |

---

## 17. Known limitations

- **Not yet tested on real devices:**
  - the macOS and Android CI jobs
  - server-mode navigation on a phone or Mac

  Run section 15 after your first install.

- **Offline editing in server mode** is not supported.
- **Concurrent-edit protection** covers fixed deposit and policy edits. Edits to
  expenses, accounts and categories keep upstream's "latest save wins".
- **Plain HTTP:** browser features that need HTTPS (for example the clipboard)
  may not work on `http://100.x:8088`.
- **CSV activity import** has no payment-method column. Backups keep the payment
  method.
- **Fixed deposit maths** does not model bank short-tenor or business-day rules.
  Enter the bank's maturity amount for exact figures.
- **Translations:** new screens for fixed deposits, policies and server mode are
  in English in every language; the payment-method labels are translated.
- **Android builds** are arm64 only.
