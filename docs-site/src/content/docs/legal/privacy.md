---
title: Privacy Policy
description: How Spool handles your data — in short, it doesn't collect any.
tableOfContents: false
lastUpdated: 2026-06-01
---

_Last updated: 1 June 2026_

Spool is a free, open-source desktop application for managing your game library
and game-save backups. It runs entirely on your own computer.

## The short version

Spool has no servers and no backend. The developer does not collect, receive,
store, transmit, or have any access to your data or your accounts. Everything
Spool does happens locally on your device or directly between your device and
online accounts that you connect yourself.

## Google user data

If you choose to connect Google Drive for cloud save-syncing, Spool requests the
**`drive.file`** scope. This grants access **only to the files Spool itself
creates** in your Google Drive — your game-save backups and small coordination
files Spool writes to track which device last played a game. Spool **cannot see,
read, or access any other file** in your Google Drive.

This access is used for one purpose only: to back up and restore your game saves
and to keep that state consistent across your own devices.

The transfer happens directly between your computer and your own Google Drive
account, performed on your machine by the bundled
[rclone](https://rclone.org/) tool. The data is **not** sent to the developer or
to any third party. The developer never receives or stores it.

## What Spool does not do

- No analytics, telemetry, tracking, or advertising.
- No selling, sharing, or transferring of any user data.
- No accounts or servers operated by the developer.

## Storage and deletion

Your save backups live in your own cloud storage and on your own devices. You
remain in full control: delete the backup files from your storage at any time,
and revoke Spool's access to a connected account whenever you like. For Google,
manage or revoke access at
<https://myaccount.google.com/permissions>.

## Limited Use disclosure

Spool's use of information received from Google APIs adheres to the
[Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy),
including the Limited Use requirements.

## Other connected services

Spool optionally integrates with services you choose to enable — for example
SteamGridDB for cover art, or other rclone cloud providers. Data you exchange
with those services is governed by their own privacy policies.

## Changes

This policy may be updated; the "Last updated" date above will change
accordingly.

## Contact

Questions: [akinzett@gmail.com](mailto:akinzett@gmail.com)
