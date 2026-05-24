# Quickstart

Sixty seconds to a running scheduler with one job firing every minute.

## 1. Start the server

```bash
RSCHED_ADMIN_PASSWORD=change-me cargo run -p rusty-sched -- server
```

Or, if you installed the binary:

```bash
RSCHED_ADMIN_PASSWORD=change-me rusty-sched server
```

The first boot creates an `admin` user, opens a SQLite DB under the OS data
dir (e.g. `~/Library/Application Support/io.rustysched.rusty-sched/rusty.db`
on macOS), and starts the HTTP server on `0.0.0.0:8080`.

## 2. Open the dashboard

Visit <http://localhost:8080>. Log in as `admin` / `change-me`.

You should see the **Jobs** tab. The header shows your username + role, a
dark/light theme toggle (☾/☀), and tabs for **Resources**, **Globals**,
**API keys**, **Users**, **Audit**.

## 3. Create a job

Fill the "Create job" form:

- **name:** `tick`
- **cron expression:** `*/1 * * * *`
- **command:** `date`

Click **Create**. Within ~60s, a run appears under the row. Click the row
to expand the run detail with a live log tail.

## 4. Trigger via CLI

Create an API key (admin → API keys tab → enter "ci" → **Create**). Copy
the plaintext token (it's only shown once).

```bash
export RSCHED_TOKEN=<paste-token>
rusty-sched cli list
rusty-sched cli trigger tick
rusty-sched cli runs tick
```

## 5. Try a condition expression

```bash
rusty-sched cli global set IS_RUN_DAY y

cat > downstream.json <<EOF
{
  "name": "downstream",
  "trigger": {"kind": "condition", "expr": "success(tick) and value(IS_RUN_DAY)"},
  "cmd": "echo downstream fired"
}
EOF
rusty-sched cli apply -f downstream.json
```

`downstream` fires on the next tick where `tick`'s most recent run is
`Success` AND the global `IS_RUN_DAY` evaluates truthy.

## What's next

- Read [Triggers](./triggers.md), [Boxes](./boxes.md), and
  [Condition DSL](./conditions.md) for the Autosys-parity surface.
- Read [JIL parser](./jil.md) if you have existing Autosys job definitions.
- Read [Deployment](./deployment.md) for systemd / launchd / Windows service
  setup.
