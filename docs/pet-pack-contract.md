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
snapshots. Those are implementation details behind the Desktop Companion. This
contract documents them for companion implementations and external displays.

## Product Model

Desktop Companion has one current desktop pet selection. Settings and the
desktop right-click menu are two controls for the same preference; they must not
create separate branches for built-in skins, Hermes PetDeX skins, and
custom-display packs.

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
- the WebUI adapter raw snapshot is the stable low-level state outlet;
- the sidecar derives the stable pet-facing data/action APIs documented below
  from that snapshot;
- Hermes PetDeX skins are a compatible future skin source, but are not exposed
  by this sidecar build yet;
- external custom display pack loading is not implemented yet;
- `GET /api/pet/capabilities` reports `custom_display_packs: false` until that
  loader exists.

## Architecture Boundary

The stable boundary has three surfaces:

```text
Hermes WebUI browser adapter
  -> raw adapter snapshot outlet
      -> Desktop Companion loopback sidecar
          -> normalized pet APIs and permission-gated actions
          -> built-in Desktop Companion display
      -> external display bridge, if a display owns its own bridge

Hermes pet assets / PetDeX
  -> bundled Desktop Companion skins or installed Hermes pets
  -> one user-facing Pet Skin selection
```

The browser adapter snapshot is a supported low-level integration surface for
advanced displays. A display that owns its own local bridge may consume that raw
snapshot directly. The Desktop Companion sidecar consumes the same snapshot and
publishes `/api/pet/attention` for displays that want a smaller normalized
shape.

Displays should not scrape Hermes WebUI DOM or read WebUI session files. They
may either consume the raw adapter snapshot they receive through their bridge,
or consume the sidecar APIs when they are using the Desktop Companion sidecar.

The sidecar owns:

- optional Hermes/WebUI attention state normalization;
- permission-gated actions such as open session, draft reply, direct send, and
  inline approval/clarify responses;
- pet skin discovery and sanitization for displays using this sidecar;
- stale/unloaded snapshot handling.

Pet packs own:

- visual assets;
- optional custom display implementation;
- how attention state is presented to the user.

Third-party displays are not required to use the Desktop Companion sidecar.
Same-machine displays may read Hermes pet assets directly when they share the
same `HERMES_HOME`. Remote or WSL setups may need a bridge process to cross the
filesystem/runtime boundary. This contract should describe those modes without
forcing them into one process topology.

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

## State Inputs

### Raw Adapter Snapshot

The WebUI adapter posts a sanitized raw snapshot to its configured local
endpoint:

```text
POST /api/webui/snapshot
```

The Desktop Companion sidecar stores the latest snapshot and exposes it back
for diagnostics or advanced consumers:

```text
GET /api/webui/snapshot
```

An external display bridge may also implement the same snapshot receiver and
parse the snapshot directly. This is the stable low-level outlet for consumers
that need richer state than `/api/pet/attention` provides.

Raw snapshot consumers should still feature-detect fields and ignore unknown
fields. The raw snapshot is intentionally broader than the normalized attention
API and may grow as the WebUI adapter learns more state.

### Normalized Attention

Displays using the Desktop Companion sidecar can poll:

```text
GET /api/pet/attention
```

This is the pet-facing view derived from the latest WebUI adapter snapshot. It
is smaller and more stable than the raw snapshot. It is a convenience view, not
a replacement for direct raw-snapshot integrations.

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

Pet displays using the Desktop Companion sidecar can read available skins with:

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

Today this endpoint returns bundled Desktop Companion skins. It may later return
installed Hermes PetDeX pets as another skin source. PetDeX-compatible entries
should be a view over Hermes pet assets, not a separate competing registry.

Hermes pets are installed under the active `HERMES_HOME` as:

```text
pets/<slug>/pet.json
pets/<slug>/spritesheet.webp
```

`pet.json` commonly contains `id`, `displayName`, `description`, and
`spritesheetPath`. Current PetDeX/Codex spritesheets use 192 x 208 frames in an
8-column x 9-row atlas. Older sheets may use a legacy row count, so a robust
skin source should derive layout from the actual spritesheet when possible.

If Desktop Companion exposes Hermes PetDeX pets through `/api/pet/skins`, it
must keep `hermes pets` as the canonical owner of PetDeX installation and
selection state. Selecting a PetDeX-backed skin should use `hermes pets select
<slug>` or the same canonical writer, rather than maintaining a conflicting
Desktop Companion registry.

Displays using this sidecar should use the sanitized `spritesheetUrl` returned
by `/api/pet/skins`, not arbitrary local file paths. Same-machine external
displays may still read `HERMES_HOME/pets` directly when they intentionally
operate in direct mode.

## Direct And Bridge Modes

There are two valid deployment shapes:

- Direct mode: Hermes, WebUI, and the desktop display share the same machine and
  `HERMES_HOME`. A display can read installed pets directly and may only need
  the WebUI adapter snapshot for activity state.
- Bridge mode: Hermes runs somewhere else, such as WSL or a remote host. A
  local bridge may be needed to move snapshots, pet assets, and selection
  commands across that boundary.

The contract does not require one universal sidecar for both modes. The open
design question is how to avoid duplicate sidecar responsibilities in WSL or
remote setups where Desktop Companion and an external display may each have a
bridge process. Future capabilities should make the active mode explicit before
we merge or replace those responsibilities.

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
- the adapter raw snapshot is a supported low-level state outlet;
- `/api/pet/capabilities`, `/api/pet/attention`, and `/api/pet/skins` are the
  stable sidecar read surface for Desktop Companion pet displays;
- Hermes PetDeX assets are compatible with the Pet Skin model, but this draft
  does not yet implement PetDeX skin discovery;
- documented fields may be added to, but should not be removed or changed
  incompatibly without a new contract version;
- displays should ignore unknown fields and check capabilities for optional
  actions.
