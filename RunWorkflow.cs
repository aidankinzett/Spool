using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using Microsoft.Toolkit.Uwp.Notifications;
using Windows.Data.Xml.Dom;
using Windows.UI.Notifications;

namespace LudusaviWrap
{
    public class RunWorkflow
    {
        private readonly string _gameName;
        private readonly string _gameExe;
        private readonly Config _config;
        private readonly PlayStateLockClient? _lockClient;
        private readonly GameEntry? _entry;
        private readonly GameLibrary? _library;

        private const string ProgressToastTag   = "ludusavi-progress";
        private const string ProgressToastGroup = "ludusavi";

        public RunWorkflow(string gameName, string gameExe, GameEntry? entry = null, GameLibrary? library = null)
        {
            _gameName = gameName;
            _gameExe  = gameExe;
            _entry    = entry;
            _library  = library;
            _config   = new Config();

            if (_config.Data.SyncServerEnabled && !string.IsNullOrEmpty(_config.Data.SyncServerUrl))
            {
                _lockClient = new PlayStateLockClient(
                    _config.Data.SyncServerUrl,
                    _config.Data.SyncServerApiKey,
                    _config.Data.DeviceId,
                    _config.Data.DeviceName);
            }
        }

        public async Task ExecuteAsync()
        {
            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show(
                    $"Ludusavi executable not found at:\n{_config.Data.LudusaviPath}\n\nPlease open settings in Ludusavi Wrap to configure it.",
                    "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string ludusavi = _config.Data.LudusaviPath;

            // 0. Check play state lock before restoring saves
            if (_lockClient != null)
            {
                ShowOrUpdateProgressToast($"Checking play state for '{_gameName}'...");
                var lockStatus = await _lockClient.CheckLockAsync(_gameName);
                if (lockStatus != null && lockStatus.Locked && lockStatus.DeviceId != _config.Data.DeviceId)
                {
                    string device = lockStatus.DeviceName ?? "another device";
                    DismissProgressToast();

                    if (!lockStatus.Stale)
                    {
                        var ans = MessageBox.Show(
                            $"{device} is currently playing '{_gameName}'.\n\nLaunch anyway?",
                            "Game Already Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                        if (ans == MessageBoxResult.No)
                            return;
                    }
                    else
                    {
                        var lastBackup = await _lockClient.GetLatestBackupEventAsync(_gameName);
                        string backupDetail = lastBackup?.Found == true
                            ? $"\n\nLast backup: {FormatTimeAgo(lastBackup.OccurredAt)} from {lastBackup.DeviceName}."
                            : "";
                        MessageBox.Show(
                            $"{device}'s session appears to have ended without releasing the lock.{backupDetail}\n\nProceeding with restore.",
                            "Stale Lock Detected", MessageBoxButton.OK, MessageBoxImage.Information);
                    }
                }
            }

            // 1. Restore saves
            ShowOrUpdateProgressToast($"Restoring saves for '{_gameName}'...");

            ProcessResult restoreResult;
            try
            {
                restoreResult = await RunProcessAsync(ludusavi, $"restore --api --cloud-sync --force \"{_gameName}\"");
            }
            catch (Exception ex)
            {
                DismissProgressToast();
                App.Log($"Failed to start Ludusavi restore for '{_gameName}': {ex}");
                MessageBox.Show($"Failed to start Ludusavi restore process:\n{ex.Message}",
                    "Ludusavi Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            var restoreOutput = ParseLudusaviOutput(restoreResult.Output);

            if (restoreOutput?.Errors?.CloudConflict != null)
            {
                DismissProgressToast();
                App.Log($"Ludusavi restore cloud conflict for '{_gameName}'");
                var ans = MessageBox.Show(
                    $"Cloud sync conflict detected for '{_gameName}'. Open Ludusavi to resolve?",
                    "Cloud Sync Conflict", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                if (ans == MessageBoxResult.Yes)
                    TryOpenLudusaviGui(ludusavi);
                return;
            }

            if (restoreOutput?.Errors?.CloudSyncFailed != null)
            {
                App.Log($"Ludusavi restore cloud sync failed for '{_gameName}' — proceeding with local saves");
                ShowToast("Cloud Sync Failed", $"Could not sync '{_gameName}' from cloud. Using local saves.");
            }

            bool noSavesToRestore = restoreOutput?.Errors?.UnknownGames?.Count > 0
                                    || restoreOutput?.Overall?.TotalGames == 0;

            if (!noSavesToRestore && restoreResult.ExitCode != 0)
            {
                DismissProgressToast();
                string details = string.IsNullOrWhiteSpace(restoreResult.Error) ? restoreResult.Output : restoreResult.Error;
                App.Log($"Ludusavi restore failed for '{_gameName}' (exit {restoreResult.ExitCode}): {details.Trim()}");
                MessageBox.Show($"Ludusavi restore failed. Game will not launch.\n\nDetails:\n{details.Trim()}",
                    "Ludusavi Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            if (_lockClient != null)
                _ = _lockClient.RecordRestoreAsync(_gameName);

            // 2. Acquire play state lock before launching
            if (_lockClient != null)
            {
                ShowOrUpdateProgressToast($"Acquiring play lock for '{_gameName}'...");
                var acquireResult = await _lockClient.AcquireLockAsync(_gameName);

                if (acquireResult.Outcome == AcquireOutcome.Conflict)
                {
                    string device = acquireResult.ConflictDeviceName ?? "another device";
                    DismissProgressToast();
                    var ans = MessageBox.Show(
                        $"{device} is currently playing '{_gameName}'.\n\nLaunch anyway?",
                        "Game Already Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                    if (ans == MessageBoxResult.No)
                        return;
                }
                else if (acquireResult.Outcome == AcquireOutcome.Unavailable)
                {
                    ShowOrUpdateProgressToast("Sync server unavailable — launching anyway...");
                }
            }

            // 3. Launch the game
            if (!File.Exists(_gameExe))
            {
                DismissProgressToast();
                MessageBox.Show($"Game executable not found at:\n{_gameExe}",
                    "Game Launcher Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            DismissProgressToast();
            ShowToast("Saves Restored", $"{_gameName} saves restored — launching game.");

            using var heartbeatCts = new CancellationTokenSource();
            Task? heartbeatTask = _lockClient != null
                ? _lockClient.StartHeartbeatLoopAsync(_gameName, heartbeatCts.Token)
                : null;

            try
            {
                await RunGameAsync(_gameExe);
            }
            catch (OperationCanceledException ex)
            {
                heartbeatCts.Cancel();
                if (heartbeatTask != null) try { await heartbeatTask; } catch { }
                App.Log($"Game launch cancelled for '{_gameName}': {ex.Message}");
                ShowToast("Launch Cancelled", "Game launch was cancelled.");
                return;
            }
            catch (Exception ex)
            {
                heartbeatCts.Cancel();
                if (heartbeatTask != null) try { await heartbeatTask; } catch { }
                App.Log($"Failed to start game '{_gameName}': {ex}");
                MessageBox.Show($"Failed to start game:\n{ex.Message}",
                    "Game Launcher Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            heartbeatCts.Cancel();
            if (heartbeatTask != null) try { await heartbeatTask; } catch { }

            // 4. Backup saves
            ShowOrUpdateProgressToast($"Backing up saves for '{_gameName}'...");

            try
            {
                var backupResult = await RunProcessAsync(ludusavi, $"backup --api --force --cloud-sync \"{_gameName}\"");
                var backupOutput = ParseLudusaviOutput(backupResult.Output);
                bool backupUnknownGame = backupOutput?.Errors?.UnknownGames?.Count > 0;

                if (backupOutput?.Errors?.CloudConflict != null)
                {
                    if (_lockClient != null) _ = _lockClient.RecordBackupAsync(_gameName);
                    DismissProgressToast();
                    App.Log($"Ludusavi backup cloud conflict for '{_gameName}'");
                    var ans = MessageBox.Show(
                        $"Cloud sync conflict detected during backup for '{_gameName}'. Your saves are backed up locally. Open Ludusavi to resolve?",
                        "Cloud Sync Conflict", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                    if (ans == MessageBoxResult.Yes)
                        TryOpenLudusaviGui(ludusavi);
                }
                else if (backupOutput?.Errors?.CloudSyncFailed != null)
                {
                    if (_lockClient != null) _ = _lockClient.RecordBackupAsync(_gameName);
                    DismissProgressToast();
                    App.Log($"Ludusavi backup cloud sync failed for '{_gameName}'");
                    ShowToast("Cloud Sync Failed", $"{_gameName} backed up locally but cloud sync failed.");
                }
                else if (backupResult.ExitCode != 0 && !backupUnknownGame)
                {
                    DismissProgressToast();
                    string details = string.IsNullOrWhiteSpace(backupResult.Error) ? backupResult.Output : backupResult.Error;
                    App.Log($"Ludusavi backup failed for '{_gameName}' (exit {backupResult.ExitCode}): {details.Trim()}");
                    MessageBox.Show($"Ludusavi backup failed. Your saves may not have been backed up.\n\nDetails:\n{details.Trim()}",
                        "Ludusavi Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
                }
                else if (backupOutput?.Overall?.TotalGames > 0)
                {
                    if (_lockClient != null) _ = _lockClient.RecordBackupAsync(_gameName);
                    DismissProgressToast();
                    ShowToast("Saves Backed Up", $"{_gameName} saves backed up successfully.");
                }
                else
                {
                    DismissProgressToast();
                    App.Log($"Ludusavi backup: no saves found for '{_gameName}'");
                }
            }
            catch (Exception ex)
            {
                DismissProgressToast();
                App.Log($"Failed to run Ludusavi backup for '{_gameName}': {ex}");
                MessageBox.Show($"Failed to run Ludusavi backup:\n{ex.Message}",
                    "Ludusavi Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
            }

            // 5. Release play state lock (fire-and-forget)
            if (_lockClient != null)
                _ = _lockClient.ReleaseLockAsync(_gameName);
        }

        private void ShowOrUpdateProgressToast(string status)
        {
            DismissProgressToast();
            try
            {
                string escapedName   = System.Security.SecurityElement.Escape(_gameName) ?? _gameName;
                string escapedStatus = System.Security.SecurityElement.Escape(status) ?? status;

                var xml = new XmlDocument();
                xml.LoadXml($"""
                    <toast>
                      <visual>
                        <binding template="ToastGeneric">
                          <text>Ludusavi Wrap</text>
                          <text>{escapedName}</text>
                          <text>{escapedStatus}</text>
                        </binding>
                      </visual>
                    </toast>
                    """);

                var toast = new ToastNotification(xml);
                toast.Tag   = ProgressToastTag;
                toast.Group = ProgressToastGroup;
                ToastNotificationManagerCompat.CreateToastNotifier().Show(toast);
            }
            catch (Exception ex)
            {
                App.Log($"Toast progress show failed: {ex.Message}");
            }
        }

        private static void DismissProgressToast()
        {
            try { ToastNotificationManagerCompat.History.Remove(ProgressToastTag, ProgressToastGroup); }
            catch { }
        }

        private static void ShowToast(string title, string body)
        {
            try
            {
                new ToastContentBuilder()
                    .AddText(title)
                    .AddText(body)
                    .Show();
            }
            catch (Exception ex)
            {
                App.Log($"Toast notification failed: {ex.Message}");
            }
        }

        private static async Task<ProcessResult> RunProcessAsync(string filename, string arguments)
        {
            var outputBuilder = new StringBuilder();
            var errorBuilder  = new StringBuilder();
            var outputClosedTcs = new TaskCompletionSource<bool>();
            var errorClosedTcs  = new TaskCompletionSource<bool>();

            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName              = filename,
                    Arguments             = arguments,
                    UseShellExecute       = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError  = true,
                    CreateNoWindow        = true
                },
                EnableRaisingEvents = true
            };

            process.OutputDataReceived += (_, e) =>
            {
                if (e.Data is null) outputClosedTcs.TrySetResult(true);
                else outputBuilder.AppendLine(e.Data);
            };
            process.ErrorDataReceived += (_, e) =>
            {
                if (e.Data is null) errorClosedTcs.TrySetResult(true);
                else errorBuilder.AppendLine(e.Data);
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            await process.WaitForExitAsync();
            await Task.WhenAll(outputClosedTcs.Task, errorClosedTcs.Task);

            var result = new ProcessResult
            {
                ExitCode = process.ExitCode,
                Output   = outputBuilder.ToString(),
                Error    = errorBuilder.ToString()
            };
            process.Dispose();
            return result;
        }

        private static LudusaviApiOutput? ParseLudusaviOutput(string stdout)
        {
            try { return JsonSerializer.Deserialize(stdout, LudusaviOutputContext.Default.LudusaviApiOutput); }
            catch { return null; }
        }

        private void TryOpenLudusaviGui(string ludusaviPath)
        {
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName       = ludusaviPath,
                    Arguments      = "gui",
                    UseShellExecute = true
                });
            }
            catch (Exception ex)
            {
                App.Log($"Failed to open Ludusavi GUI: {ex}");
                MessageBox.Show($"Failed to open Ludusavi GUI:\n{ex.Message}",
                    "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async Task RunGameAsync(string exePath)
        {
            var tcs      = new TaskCompletionSource<int>();
            bool runAsAdmin = (_entry?.RunAsAdmin == true) || RegistryHelper.GetCompatFlagRunAsAdmin(exePath);

            var startInfo = new ProcessStartInfo
            {
                FileName         = exePath,
                WorkingDirectory = Path.GetDirectoryName(exePath) ?? "",
                UseShellExecute  = true
            };
            if (runAsAdmin)
                startInfo.Verb = "runas";

            var process = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
            process.Exited += (_, _) => { tcs.SetResult(process.ExitCode); process.Dispose(); };

            try
            {
                process.Start();
            }
            catch (System.ComponentModel.Win32Exception ex) when (ex.NativeErrorCode == 1223)
            {
                throw new OperationCanceledException("Game launch was cancelled (Administrator elevation was denied).", ex);
            }

            await tcs.Task;
        }

        private static string FormatTimeAgo(string? isoTimestamp)
        {
            if (string.IsNullOrEmpty(isoTimestamp) ||
                !DateTimeOffset.TryParse(isoTimestamp, out var dt))
                return "unknown time ago";

            var elapsed = DateTimeOffset.UtcNow - dt.ToUniversalTime();
            if (elapsed.TotalMinutes < 2)  return "just now";
            if (elapsed.TotalMinutes < 60) return $"{(int)elapsed.TotalMinutes}m ago";
            if (elapsed.TotalHours   < 24) return $"{(int)elapsed.TotalHours}h ago";
            return $"{(int)elapsed.TotalDays}d ago";
        }
    }

    public class ProcessResult
    {
        public int    ExitCode { get; set; }
        public string Output   { get; set; } = "";
        public string Error    { get; set; } = "";
    }

    public class LudusaviApiOutput
    {
        [JsonPropertyName("errors")]
        public LudusaviApiErrors? Errors { get; set; }

        [JsonPropertyName("overall")]
        public LudusaviApiOverall? Overall { get; set; }
    }

    public class LudusaviApiErrors
    {
        [JsonPropertyName("unknownGames")]
        public List<string>? UnknownGames { get; set; }

        [JsonPropertyName("cloudConflict")]
        public System.Text.Json.JsonElement? CloudConflict { get; set; }

        [JsonPropertyName("cloudSyncFailed")]
        public System.Text.Json.JsonElement? CloudSyncFailed { get; set; }
    }

    public class LudusaviApiOverall
    {
        [JsonPropertyName("totalGames")]
        public int TotalGames { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = false)]
    [JsonSerializable(typeof(LudusaviApiOutput))]
    internal partial class LudusaviOutputContext : JsonSerializerContext { }
}
