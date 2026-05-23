using System;
using System.Diagnostics;
using System.IO;
using System.Threading.Tasks;
using System.Windows;

namespace LudusaviWrap
{
    public partial class RunWindow : Window
    {
        private readonly string _gameName;
        private readonly string _gameExe;
        private readonly Config _config;

        public RunWindow(string gameName, string gameExe)
        {
            InitializeComponent();
            _gameName = gameName;
            _gameExe = gameExe;
            _config = new Config();

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
                Application.Current.Shutdown();
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

            if (restoreResult.ExitCode != 0)
            {
                MessageBox.Show($"Ludusavi restore failed. Game will not launch.\n\nDetails:\n{restoreResult.Output}\n{restoreResult.Error}",
                                "Ludusavi Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            // Check for cloud conflicts
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

            // 2. Launch the game
            if (!File.Exists(_gameExe))
            {
                MessageBox.Show($"Game executable not found at:\n{_gameExe}", "Game Launcher Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            Hide();

            try
            {
                await RunGameAsync(_gameExe);
            }
            catch (Exception ex)
            {
                Show();
                MessageBox.Show($"Failed to start game:\n{ex.Message}", "Game Launcher Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            Show();

            // 3. Backup saves
            StatusLabel.Text = $"Backing up saves for '{_gameName}'...";

            try
            {
                var backupResult = await RunProcessAsync(ludusavi, $"backup --force --cloud-sync \"{_gameName}\"");
                if (backupResult.ExitCode != 0)
                {
                    MessageBox.Show("Ludusavi backup failed. Your saves may not have been uploaded to the cloud.",
                                    "Ludusavi Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to run Ludusavi backup:\n{ex.Message}", "Ludusavi Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
            }
        }

        private async Task<ProcessResult> RunProcessAsync(string filename, string arguments)
        {
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
                }
            };

            process.Start();

            var outputTask = process.StandardOutput.ReadToEndAsync();
            var errorTask = process.StandardError.ReadToEndAsync();

            await process.WaitForExitAsync();

            string output = await outputTask;
            string error = await errorTask;

            var result = new ProcessResult
            {
                ExitCode = process.ExitCode,
                Output = output,
                Error = error
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
