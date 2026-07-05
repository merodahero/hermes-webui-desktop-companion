# Loopback Protocol

The initial protocol is HTTP/JSON on loopback.

Default endpoint:

```text
http://127.0.0.1:17787
```

## `GET /health`

Returns:

```json
{
  "ok": true,
  "service": "hermes-webui-desktop-companion",
  "status": "ok",
  "name": "Hermes WebUI Desktop Companion",
  "version": "0.1.0",
  "sidecar": {
    "type": "loopback",
    "health_path": "/health"
  },
  "management": {
    "version": 1,
    "install": {
      "kind": "external_url",
      "label": "Install Desktop Companion",
      "url": "https://github.com/franksong2702/hermes-webui-desktop-companion#first-time-setup"
    },
    "start": {
      "kind": "deep_link",
      "label": "Start Desktop Companion",
      "uri": "hermes-desktop-companion://start",
      "fallback_command": "npm run start:pet"
    },
    "manager": {
      "kind": "deep_link",
      "label": "Open Pet Gallery",
      "uri": "hermes-desktop-companion://gallery",
      "path": "/pet/gallery",
      "requires": ["native-host"]
    },
    "health": {
      "path": "/health"
    },
    "actions": [
      {
        "id": "install_desktop_companion",
        "kind": "external_url",
        "label": "Install Desktop Companion",
        "url": "https://github.com/franksong2702/hermes-webui-desktop-companion#first-time-setup"
      },
      {
        "id": "start_desktop_companion",
        "kind": "deep_link",
        "label": "Start Desktop Companion",
        "uri": "hermes-desktop-companion://start",
        "fallback_command": "npm run start:pet"
      },
      {
        "id": "open_pet_gallery",
        "kind": "deep_link",
        "label": "Open Pet Gallery",
        "uri": "hermes-desktop-companion://gallery",
        "path": "/pet/gallery",
        "requires": ["native-host"]
      }
    ]
  },
  "runtime": {
    "sidecar": "running",
    "native_host": "running",
    "bridge": "connected",
    "last_seen_at": 1782477600,
    "webui_origin": "http://127.0.0.1:8787",
    "native_host_registered_at": 1782477580
  }
}
```

The `ok`, `service`, and `version` fields are retained for simple scripts. The
`status`, `name`, and `sidecar` fields are intended for future WebUI extension
settings or diagnostics panels.

The optional `runtime` object is machine-readable status for generic
sidecar/native extensions. `sidecar` reports the loopback process state,
`native_host` reports the native app state (`running`, `stopped`, or
`not_registered`), and `bridge` reports whether the WebUI adapter has sent a
fresh snapshot (`connected`, `waiting`, `stale`, or `unloaded`). `last_seen_at`
is the latest WebUI snapshot timestamp in Unix seconds when available.

The optional `management` object describes user-facing management affordances
for WebUI extension settings. `install` is an external trusted-local setup link,
`start` is an operating-system deep link for the installed native app, and
`manager` is a deep link that starts Desktop Companion if needed and then opens
Pet Gallery. `fallback_command` is for local development docs and should not be
executed silently by WebUI. These fields are descriptive contract metadata; they
do not grant WebUI permission to silently install native code or launch a
process.

## `POST /api/webui/snapshot`

Receives a sanitized WebUI browser snapshot.

Example request:

```json
{
  "source": "hermes-webui",
  "version": 1,
  "timestamp": "2026-06-14T00:00:00.000Z",
  "page": {
    "href": "http://127.0.0.1:8787/",
    "pathname": "/",
    "visibilityState": "visible"
  },
  "capabilities": {
    "inPagePet": false,
    "loopback": true,
    "canReceiveActions": true
  },
  "companion": {
    "skin": "keeper",
    "skinName": "May",
    "state": "idle",
    "collapsed": false,
    "attentionCount": 0,
    "attention": []
  }
}
```

Returns:

```json
{
  "ok": true
}
```

## `GET /api/webui/snapshot`

Returns the latest received snapshot:

```json
{
  "ok": true,
  "snapshot": {}
}
```

## `GET /api/pet/navigation`

Returns the next pending desktop-pet navigation command for the WebUI adapter.
This mirrors the original Hermes WebUI desktop pet bridge. The adapter passes
the last consumed command id as `since`, acknowledges the returned command, then
uses WebUI's in-page session loader when available.

```json
{
  "ok": true,
  "command": {
    "id": "mabc1234-deadbeef",
    "session_id": "abc123",
    "url": "http://127.0.0.1:8787/session/abc123"
  }
}
```

## `POST /api/pet/navigation_ack`

Acknowledges a pending navigation command:

```json
{
  "id": "mabc1234-deadbeef"
}
```

## `GET /api/pet/actions`

Returns the next pending desktop-pet action command for the WebUI adapter. These
commands are used for WebUI write operations that must run inside the browser's
authenticated Hermes WebUI origin, such as approval and clarify responses.

```json
{
  "ok": true,
  "command": {
    "id": "mabc1234-deadbeef",
    "type": "approval.respond",
    "session_id": "abc123",
    "body": {
      "session_id": "abc123",
      "choice": "once",
      "approval_id": "approval-1"
    }
  }
}
```

## `POST /api/pet/action_ack`

Acknowledges a pending action command after the WebUI adapter has executed it:

```json
{
  "id": "mabc1234-deadbeef",
  "ok": true,
  "status": 200,
  "result": {
    "ok": true
  }
}
```

The desktop pet can submit approval and clarify actions to the sidecar using the
same relative routes it used in the original WebUI-hosted pet:

- `POST /api/approval/respond`
- `POST /api/clarify/respond`

The sidecar queues those requests and waits for the WebUI adapter to execute
them. It does not call Hermes WebUI write APIs directly from the loopback
sidecar. If the browser adapter successfully executes a WebUI action and then
crashes before posting `action_ack`, the sidecar may time out and allow a retry.
That rare crash window can duplicate an action, so action handlers should stay
idempotent where WebUI supports it.

Inline approval and clarify responses are gated by the sidecar preference
`allow_inline_action_responses`, which defaults to `false`. When disabled, these
routes return `403 inline_action_responses_disabled`; the desktop pet can then
surface a user confirmation card or open the session in WebUI.

## `GET /api/pet/attention`

Returns the current desktop-pet attention rows derived from the latest WebUI
extension snapshot.

```json
{
  "ok": true,
  "sessions": [],
  "source": "webui-extension-snapshot"
}
```

When no WebUI page has reported a snapshot yet, `sessions` is empty and
`source` is `empty`.

Snapshots marked with `reason: "unload"` and timestamped snapshots older than
30 seconds do not produce attention rows. The sidecar may still keep the latest
snapshot for diagnostics or loopback origin discovery, but stale browser state
must not keep a desktop bubble alive after the WebUI page has unloaded or stopped
reporting.

The standalone extension does not infer completed-session attention from old
message counts without a WebUI unread baseline. Attention rows are derived from
current WebUI frontend state:

- running rows use `/api/session` runtime journal snapshots, `activity_scene_v1`,
  and live `INFLIGHT` state for process text;
- ready rows use a fresh WebUI `hermes-session-completion-unread` marker or an
  adapter-observed `running` to `idle` completion transition from the current
  browser session. Historical completed sessions do not appear just because
  their old `viewed_counts` baseline is lower than their message count, and old
  completion-unread markers are ignored by the desktop companion;
- approval and clarification rows use attention metadata exposed by WebUI
  session rows.

## `GET /api/pet/capabilities`

Returns the current Desktop Companion Pet Pack contract version, the stable
pet-facing endpoints, attention status/source values, user-facing pet selection
model, and optional action capabilities.

```json
{
  "ok": true,
  "pet_pack_contract_version": 1,
  "service": "hermes-webui-desktop-companion",
  "version": "0.1.0",
  "pet_model": {
    "user_facing_selection": "pet_skin",
    "pack_types": ["classic_skin", "custom_display_pack"],
    "default_pack_type": "classic_skin",
    "custom_display_packs": false
  },
  "endpoints": {
    "health": "/health",
    "snapshot": "/api/webui/snapshot",
    "attention": "/api/pet/attention",
    "skins": "/api/pet/skins",
    "capabilities": "/api/pet/capabilities",
    "open_session": "/api/pet/open_session",
    "preferences": "/api/pet/preference"
  },
  "attention": {
    "statuses": ["running", "ready", "action_required"],
    "sources": ["webui-extension-snapshot", "empty", "stale", "unloaded"],
    "stale_after_ms": 30000
  },
  "capabilities": {
    "read_attention": true,
    "read_snapshot": true,
    "read_skins": true,
    "open_session": true,
    "draft_reply": true,
    "direct_send": false,
    "inline_action_responses": false
  }
}
```

See `docs/pet-pack-contract.md` for the Pet Pack contract. Simple displays can
use `/api/pet/attention`; advanced displays may consume the raw adapter
snapshot directly when they own their own bridge. `custom_display_packs: false`
means the contract reserves the model, but this sidecar build does not yet load
external custom display packs.
## `GET /api/pet/skins`

Lists Desktop Companion's bundled skins plus installed Hermes pets from
`<HERMES_HOME>/pets`, defaulting to `~/.hermes/pets`.

Hermes pet entries use `id: "hermes-<slug>"` and expose their spritesheet
through `/api/pet/hermes-pets/<slug>/<spritesheet>`. Current 8-column × 9-row
Petdex sheets and older 9-column × 8-row sheets are both mapped into the Desktop
Companion state names expected by the pet renderer.

## `GET /api/pet/gallery`

Searches Petdex's live search API and annotates each result with local install
state. If live search is unavailable, the sidecar falls back to the static
Petdex manifest. Results are paginated with `offset` and `limit`; `q` filters
by slug, display name, kind, or submitter. Compatibility is `unchecked` before install,
`supported` when the installed spritesheet is renderable by Desktop Companion,
and `unsupported` when the pet exists locally but does not match a supported
spritesheet layout.

## `POST /api/pet/gallery/install`

Installs a Petdex pet by delegating to the local `hermes pets install <slug>`
command. If the CLI fails because its static Petdex manifest has not caught up
with a live approved pet, the sidecar fetches `/api/install-pet/<slug>`,
downloads the pet JSON and spritesheet, and writes them into
`<HERMES_HOME>/pets/<slug>`. The sidecar then re-reads the installed pet and
returns the installed state plus a renderable skin manifest when the spritesheet
is supported.

```json
{
  "slug": "panam"
}
```

## `POST /api/pet/gallery/remove`

Removes an installed Hermes pet by delegating to
`hermes pets remove <slug>`. Bundled Desktop Companion skins are not removable
through this route.

## `POST /api/pet/open_webui`

Surfaces the latest Hermes WebUI browser page reported by the adapter. If no
WebUI page has reported a snapshot yet, the sidecar falls back to the configured
WebUI base URL from `HERMES_DESKTOP_COMPANION_WEBUI_BASE`, defaulting to
`http://127.0.0.1:8787/`.

On macOS, the sidecar first looks for an existing Google Chrome tab on the same
WebUI loopback origin and focuses/reuses it. If no matching tab is found, it
falls back to opening the target WebUI URL normally.

This route is used by clicking the desktop pet body. It does not queue a
navigation command, change sessions, or apply composer draft/autosend behavior.

## `POST /api/pet/open_session`

Queues an `open_session` command for the WebUI adapter and asks the operating
system to surface the browser. On macOS, the sidecar first looks for an existing
Google Chrome tab on the same WebUI loopback origin and switches that tab to the
target session. If no existing tab is found, it falls back to opening the target
WebUI session URL normally. The bridge command remains available for in-page
handling such as draft/autosend support.

When `draft` or `autosend` is present, the sidecar waits for the WebUI adapter to
acknowledge the navigation command before reporting success, because the browser
must apply the composer draft inside Hermes WebUI before the desktop reply can
be considered handled.

Direct autosend is gated by the sidecar preference `allow_direct_send`, which
defaults to `false`. If a command requests `autosend` while that permission is
off, the sidecar keeps the draft but downgrades `autosend` to `false` and marks
the queued command with `autosend_blocked: true`.

```json
{
  "session_id": "abc123"
}
```

Returns:

```json
{
  "ok": true,
  "queued": true,
  "focused": true,
  "reused": true,
  "opened": false,
  "url": "http://127.0.0.1:8787/session/abc123"
}
```

## `GET /pet`

Serves the transparent Tauri pet-window page.

## `GET /pet/bubbles`

Serves the companion bubble-window page.

## Compatibility

Fields may be added over time. Existing fields should remain backwards
compatible unless the protocol version changes.

## Upstream extension fit

This protocol intentionally lives outside Hermes WebUI core. The current WebUI
extension sends snapshots to a trusted loopback sidecar because the upstream
extension surface supports same-origin static assets and browser APIs, but not a
formal extension backend route yet.

If Hermes WebUI later lands an official extension backend bridge, the companion
should adapt this protocol to that bridge instead of adding WebUI core routes
for desktop-only behavior.
