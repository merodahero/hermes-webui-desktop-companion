# Desktop Companion Pet Pack Contract v1

This document defines the product and technical boundary for Desktop Companion
pet skins and pet packs.

The user-facing model is intentionally simple:

```text
Install Desktop Companion once.
Choose a Pet Skin.
Desktop Companion decides how that skin is displayed.
```

Users should not need to understand adapters, sidecars, renderers, or raw WebUI
snapshots. Those are implementation details behind the Desktop Companion.

## Product Model

Desktop Companion has one current desktop pet selection. Settings and the
desktop right-click menu are two controls for the same preference; they must not
create separate branches for built-in skins and custom-display packs.

The primary user-facing control is:

```text
Pet Skin
```

A pet skin may be backed by one of two pack shapes:

- `classic_skin`: sprite/assets that use the built-in Desktop Companion display.
- `custom_display_pack`: a pet pack that brings its own display implementation.

In both cases, the user still chooses a pet skin. A custom display is selected
because the active skin/pack declares that it needs one, not because the user is
asked to pick a "renderer" as a separate product concept.

Current implementation status:

- bundled classic skins are supported today;
- the sidecar exposes the stable pet-facing data/action APIs documented below;
- external custom display pack loading is not implemented yet;
- `GET /api/pet/capabilities` reports `custom_display_packs: false` until that
  loader exists.

## Architecture Boundary

The stable boundary is:

```text
Hermes WebUI browser adapter
  -> Desktop Companion loopback sidecar
  -> pet-facing sidecar APIs
  -> built-in display or custom display pack
```

Pet packs and displays should consume the sidecar APIs instead of scraping
Hermes WebUI DOM, reading WebUI session files, or running Hermes commands
directly.

The sidecar owns:

- Hermes/WebUI attention state normalization;
- permission-gated actions such as open session, draft reply, direct send, and
  inline approval/clarify responses;
- pet skin discovery and sanitization;
- stale/unloaded snapshot handling.

Pet packs own:

- visual assets;
- optional custom display implementation;
- how attention state is presented to the user.

## Discovery

Pet-aware clients can discover the supported contract and endpoints with:

```text
GET /api/pet/capabilities
```

Example response:

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
  },
  "server_time": 1783039000
}
```

`direct_send` and `inline_action_responses` are preference-gated. A display
must read the current capability value instead of assuming those actions are
enabled.

## Preferred Pet Input

Pet displays should poll:

```text
GET /api/pet/attention
```

This is the pet-facing view derived from the latest WebUI adapter snapshot. It
is smaller and more stable than the raw snapshot.

Example `running` response:

```json
{
  "ok": true,
  "source": "webui-extension-snapshot",
  "sessions": [
    {
      "session_id": "abc123",
      "status": "running",
      "title": "Build release notes",
      "text": "Writing summary",
      "process_text": "Writing summary",
      "message_count": 8,
      "last_message_at": 1783038900,
      "updated_at": 1783038910,
      "started_at": 1783038800,
      "action_required": false,
      "action_required_type": "",
      "action_required_key": "",
      "action_required_count": 0,
      "action_required_command": "",
      "action_required_description": "",
      "action_required_approval_id": "",
      "action_required_choices": [],
      "action_required_clarify_id": ""
    }
  ],
  "server_time": 1783039000
}
```

Example `ready` response:

```json
{
  "ok": true,
  "source": "webui-extension-snapshot",
  "sessions": [
    {
      "session_id": "def456",
      "status": "ready",
      "title": "Review PR #29",
      "text": "Ready to review",
      "process_text": "Ready to review",
      "message_count": 12,
      "last_message_at": 1783038900,
      "updated_at": 1783038905,
      "started_at": 1783038200,
      "action_required": false,
      "action_required_count": 0
    }
  ],
  "server_time": 1783039000
}
```

Example `action_required` response:

```json
{
  "ok": true,
  "source": "webui-extension-snapshot",
  "sessions": [
    {
      "session_id": "ghi789",
      "status": "action_required",
      "title": "Deploy 8787 bundle",
      "text": "Approve command",
      "process_text": "Approve command",
      "message_count": 5,
      "action_required": true,
      "action_required_type": "approval",
      "action_required_key": "approval-1",
      "action_required_count": 1,
      "action_required_command": "npm test",
      "action_required_description": "Approve command",
      "action_required_approval_id": "approval-1",
      "action_required_choices": []
    }
  ],
  "server_time": 1783039000
}
```

## Empty, Stale, And Unloaded State

When no browser adapter has posted a snapshot yet:

```json
{
  "ok": true,
  "sessions": [],
  "source": "empty"
}
```

When the latest snapshot is older than `stale_after_ms`, the sidecar returns:

```json
{
  "ok": true,
  "sessions": [],
  "source": "stale"
}
```

When the browser adapter reports `reason: "unload"`, the sidecar returns:

```json
{
  "ok": true,
  "sessions": [],
  "source": "unloaded"
}
```

Displays must treat `empty`, `stale`, and `unloaded` as no active pet
attention. They may still show an idle pet, but should not keep old bubbles,
badges, or attention state alive from a stale snapshot.

## Attention Fields

Stable v1 fields in each attention row:

- `session_id`: Hermes WebUI session id.
- `status`: one of `running`, `ready`, or `action_required`.
- `title`: short display title.
- `text`: concise user-facing row text.
- `process_text`: concise process/status text. It may equal `text`.
- `message_count`: current WebUI message count for the session.
- `last_message_at`: last message timestamp in Unix seconds when available.
- `updated_at`: session/update timestamp in Unix seconds when available.
- `started_at`: session start timestamp in Unix seconds when available.
- `action_required`: true when the row requires user action.
- `action_required_type`: action category, such as `approval` or `clarify`.
- `action_required_key`: stable key for dismissing/deduping the action row.
- `action_required_count`: pending action count.
- `action_required_command`: command text when an approval asks about a command.
- `action_required_description`: short description to show the user.
- `action_required_approval_id`: approval id for approval responses.
- `action_required_choices`: clarify choices when available.
- `action_required_clarify_id`: clarify prompt id when available.

Displays should ignore unknown fields so v1 can grow additively.

## Pet Skins

Pet displays can read bundled Desktop Companion skins with:

```text
GET /api/pet/skins
```

Example response:

```json
{
  "ok": true,
  "skins": [
    {
      "id": "keeper",
      "displayName": "May",
      "description": "Default companion pet",
      "spritesheetUrl": "/extensions/pets/keeper/spritesheet.webp"
    }
  ]
}
```

The sidecar owns skin discovery and sanitization. Displays should not run
`hermes pets`, read petdex files directly, or load arbitrary local file paths.
If Desktop Companion later uses Hermes petdex as a skin source, this endpoint
remains the stable boundary.

## Raw Snapshot

Advanced displays may read the raw adapter snapshot for diagnostics or
experiments:

```text
GET /api/webui/snapshot
```

However, raw snapshot shape is broader than the pet pack contract. Prefer
`/api/pet/attention` for ordinary pet behavior and rely only on fields
documented in this contract unless a future contract version promotes more
fields.

## Actions

The v1 contract includes optional action capabilities:

- Open a session: `POST /api/pet/open_session`.
- Open a session with a draft reply: `POST /api/pet/open_session` with `draft`.
- Request direct autosend: `POST /api/pet/open_session` with `draft` and
  `autosend`.
- Inline approval and clarify responses through the existing action routes.

Action support is preference-gated. Displays must read
`GET /api/pet/capabilities` or `GET /api/pet/preference` and handle disabled
actions gracefully.

The sidecar queues WebUI write actions for the browser adapter to execute
inside the authenticated Hermes WebUI origin. Pet packs and displays should not
call Hermes WebUI authenticated write APIs directly.

## Versioning

`pet_pack_contract_version: 1` means:

- the user-facing selection model is "Pet Skin";
- `/api/pet/capabilities`, `/api/pet/attention`, and `/api/pet/skins` are the
  stable read surface for Desktop Companion pet displays;
- documented fields may be added to, but should not be removed or changed
  incompatibly without a new contract version;
- displays should ignore unknown fields and check capabilities for optional
  actions.
