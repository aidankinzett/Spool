using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using FluentWindow = Wpf.Ui.Controls.FluentWindow;

namespace LudusaviWrap
{
    public partial class RunWindow : FluentWindow
    {
        private readonly string _gameName;
        private readonly string _gameExe;
        private readonly Config _config;
        private PlayStateLockClient? _lockClient;
        private readonly bool _exitAppOnFinish;

        public RunWindow(string gameName, string gameExe, bool exitAppOnFinish = true)
        {
            InitializeComponent();
            _gameName = gameName;
            _gameExe = gameExe;
            _exitAppOnFinish = exitAppOnFinish;
            _config = new Config();

            if (_config.Data.SyncServerEnabled && !string.IsNullOrEmpty(_config.Data.SyncServerUrl))
            {
                _lockClient = new PlayStateLockClient(
                    _config.Data.SyncServerUrl,
                    _config.Data.SyncServerApiKey,
                    _config.Data.DeviceId,
                    _config.Data.DeviceName);
            }

            Loaded += RunWindow_Loaded;
        }

        private async void RunWindow_Loaded(object sender, RoutedEventArgs e)
        {
            try
            {
                await ExecuteWorkflowAsync();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"An unexpected error occurred: {ex.Message}", "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            finally
            {
                if (_exitAppOnFinish)
                {
                    Application.Current.Shutdown();
                }
                else
                {
                    try
                    {
                        Close();
                        Application.Current.MainWindow?.Show();
                    }
                    catch { }
                }
            }
        }

        private async Task ExecuteWorkflowAsync()
        {
            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show($"Ludusavi executable not found at:\n{_config.Data.LudusaviPath}\n\nPlease open settings in Ludusavi Wrap to configure it.",
                                "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string ludusavi = _config.Data.LudusaviPath;

            // 0. Check play state lock before restoring saves
            if (_lockClient != null)
            {
                StatusLabel.Text = $"Checking play state for '{_gameName}'...";
                var lockStatus = await _lockClient.CheckLockAsync(_gameName);
                if (lockStatus != null && lockStatus.Locked && !lockStatus.Stale &&
                    lockStatus.DeviceId != _config.Data.DeviceId)
                {
                    string device = lockStatus.DeviceName ?? "another device";
                    var ans = MessageBox.Show(
                        $"{device} is currently playing '{_gameName}'.\n\nLaunch anyway?",
                        "Game Already Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                    if (ans == MessageBoxResult.No)
                        return;
                }
            }

            // 1. Restore saves
            StatusLabel.Text = $"Restoring saves for '{_gameName}'...";

            ProcessResult restoreResult;
            try
            {
                restoreResult = await RunProcessAsync(ludusavi, $"restore --api --cloud-sync --force \"{_gameName}\"");
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to start Ludusavi restore process:\n{ex.Message}", "Ludusavi Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            // Check for cloud conflicts before exit code — a conflict may itself cause a non-zero exit
            string combinedOutput = restoreResult.Output + restoreResult.Error;
            if (combinedOutput.Contains("cloudConflict") || combinedOutput.Contains("cloudSyncFailed"))
            {
                var ans = MessageBox.Show($"Cloud sync conflict detected for '{_gameName}'. Open Ludusavi to resolve?",
                                          "Ludusavi - Cloud Sync Conflict", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                if (ans == MessageBoxResult.Yes)
                {
                    try
                    {
                        Process.Start(new ProcessStartInfo
                        {
                            FileName = ludusavi,
                            Arguments = "gui",
                            UseShellExecute = true
                        });
                    }
                    catch (Exception ex)
                    {
                        MessageBox.Show($"Failed to open Ludusavi GUI:\n{ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                    }
                }
                return;
            }

            if (restoreResult.ExitCode != 0)
            {
                // Prefer stderr for human-readable details; fall back to stdout (JSON when --api is used)
                string details = string.IsNullOrWhiteSpace(restoreResult.Error) ? restoreResult.Output : restoreResult.Error;
                MessageBox.Show($"Ludusavi restore failed. Game will not launch.\n\nDetails:\n{details.Trim()}",
                                "Ludusavi Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            // 2. Acquire play state lock before launching
            if (_lockClient != null)
            {
                StatusLabel.Text = $"Acquiring play lock for '{_gameName}'...";
                var acquireResult = await _lockClient.AcquireLockAsync(_gameName);

                if (acquireResult.Outcome == AcquireOutcome.Conflict)
                {
                    string device = acquireResult.ConflictDeviceName ?? "another device";
                    var ans = MessageBox.Show(
                        $"{device} is currently playing '{_gameName}'.\n\nLaunch anyway?",
                        "Game Already Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                    if (ans == MessageBoxResult.No)
                        return;
                }
                else if (acquireResult.Outcome == AcquireOutcome.Unavailable)
                {
                    StatusLabel.Text = $"Sync server unavailable — launching anyway...";
                }
            }

            // 3. Launch the game
            if (!File.Exists(_gameExe))
            {
                MessageBox.Show($"Game executable not found at:\n{_gameExe}", "Game Launcher Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            Hide();

            using var heartbeatCts = new CancellationTokenSource();
            Task? heartbeatTask = _lockClient != null
                ? _lockClient.StartHeartbeatLoopAsync(_gameName, heartbeatCts.Token)
                : null;

            try
            {
                await RunGameAsync(_gameExe);
            }
            catch (Exception ex)
            {
                heartbeatCts.Cancel();
                if (heartbeatTask != null) try { await heartbeatTask; } catch { }
                Show();
                MessageBox.Show($"Failed to start game:\n{ex.Message}", "Game Launcher Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            heartbeatCts.Cancel();
            if (heartbeatTask != null) try { await heartbeatTask; } catch { }

            Show();

            // 4. Backup saves
            StatusLabel.Text = $"Backing up saves for '{_gameName}'...";

            try
            {
                var backupResult = await RunProcessAsync(ludusavi, $"backup --force --cloud-sync \"{_gameName}\"");
                string backupCombined = backupResult.Output + backupResult.Error;
                if (backupCombined.Contains("cloudConflict") || backupCombined.Contains("cloudSyncFailed"))
                {
                    MessageBox.Show($"Cloud sync conflict detected during backup for '{_gameName}'. Your saves are backed up locally but may not be synced to the cloud. Open Ludusavi to resolve.",
                                    "Ludusavi - Cloud Sync Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
                }
                else if (backupResult.ExitCode != 0)
                {
                    string details = string.IsNullOrWhiteSpace(backupResult.Error) ? backupResult.Output : backupResult.Error;
                    MessageBox.Show($"Ludusavi backup failed. Your saves may not have been backed up.\n\nDetails:\n{details.Trim()}",
                                    "Ludusavi Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to run Ludusavi backup:\n{ex.Message}", "Ludusavi Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
            }

            // 5. Release play state lock (fire-and-forget)
            if (_lockClient != null)
                _ = _lockClient.ReleaseLockAsync(_gameName);
        }

        private async Task<ProcessResult> RunProcessAsync(string filename, string arguments)
        {
            var outputBuilder = new StringBuilder();
            var errorBuilder = new StringBuilder();
            var outputClosedTcs = new TaskCompletionSource<bool>();
            var errorClosedTcs = new TaskCompletionSource<bool>();

            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = filename,
                    Arguments = arguments,
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    CreateNoWindow = true
                },
                EnableRaisingEvents = true
            };

            process.OutputDataReceived += (sender, e) =>
            {
                if (e.Data is null) outputClosedTcs.TrySetResult(true);
                else outputBuilder.AppendLine(e.Data);
            };

            process.ErrorDataReceived += (sender, e) =>
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
                Output = outputBuilder.ToString(),
                Error = errorBuilder.ToString()
            };

            process.Dispose();
            return result;
        }

        private async Task RunGameAsync(string exePath)
        {
            var tcs = new TaskCompletionSource<int>();
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = exePath,
                    WorkingDirectory = Path.GetDirectoryName(exePath) ?? ""
                },
                EnableRaisingEvents = true
            };

            process.Exited += (sender, args) =>
            {
                tcs.SetResult(process.ExitCode);
                process.Dispose();
            };

            process.Start();
            await tcs.Task;
        }
    }

    public class ProcessResult
    {
        public int ExitCode { get; set; }
        public string Output { get; set; } = "";
        public string Error { get; set; } = "";
    }
}
