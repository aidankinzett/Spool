# Upload Download Hub

_Started 2026-05-28 04:13 UTC_

---

## User

in the redesign can we have one central location for all uploads and downloads like how steam has?

## Assistant

_[tool: list_files]_

_[tool: read_file]_

_[tool: list_files]_

_[tool: read_file]_

_[tool: read_file]_

## User

it looks like we already do, should we add uploads in there?

## Assistant

_[tool: grep]_

_[tool: grep]_

Yes — adding uploads to the same panel makes sense. Right now Spool has a downloads pill + panel in the title bar (mirroring Steam's downloads tray), but for a save-sync app, uploads are arguably just as common: save backups pushing to the sync server, install shares heading out to a LAN peer. Putting both in one "Transfers" surface keeps the mental model simple.

Let me update the panel to show both directions.

_[tool: str_replace_edit]_

Now I'll add uploads to the central transfers panel. I'll generalize the data model so a "transfer" can be in or out, then split the panel into two clearly-labeled sections.Now update the pill to show both directions, and the panel to have two sections.

_[tool: str_replace_edit]_

Now restructure the panel into a TransfersPanel with two sections, and update DownloadRow to use the generic peer/dir fields.

_[tool: str_replace_edit]_

Now update `DownloadRow` to handle both directions and use the new `peer`/`peerKind` fields, plus update the kind labels (saves vs install vs extracting).

_[tool: str_replace_edit]_

Now add the direction arrow to ProgressBar's footer (so a row knows it's going to vs. from the peer):

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

Now update LibraryWithProgressScene + DownloadingInlineBlock + Object.assign to use the new fields, and update the standalone-panel artboard in Spool Redesign.html to use TransfersPanel:

_[tool: grep]_

_[tool: str_replace_edit]_

Now update LibraryWithProgressScene to pass the right props to TransferPill and DownloadPanel:

_[tool: read_file]_

_[tool: str_replace_edit]_

Now update the Object.assign at the bottom and the toast position:

_[tool: read_file]_

_[tool: str_replace_edit]_

_[tool: str_replace_edit]_

Now update the standalone-panel artboard in Spool Redesign.html — relabel and resize:

_[tool: str_replace_edit]_

_[tool: done]_

_[tool: fork_verifier_agent]_

The transfers panel now hosts both directions in one place:

- **Title-bar pill** has two arms — ↓ downloads count + progress, ↑ uploads count + progress. An idle side dims so the active direction reads clearly at a glance.
- **Panel** opens into two stacked sections: *Downloading* (game installs from LAN/TorBox/peer) and *Uploading* (save backups going to sync, install shares going out to a peer). Each row keeps the cassette progress strip, with a ↑/↓ arrow before the peer name.
- **Footer** rolls combined throughput and free space across both directions.

You can find it as the `dl-panel` artboard (standalone) and in the `lib-active` library scene. Backwards-compat aliases (`DownloadPanel`, `DownloadPill`, `DownloadRow`) still exist so any other artboards referencing them keep working.Done — the transfers panel now centralizes both downloads and uploads in one Steam-style surface, accessible from the title-bar pill.

