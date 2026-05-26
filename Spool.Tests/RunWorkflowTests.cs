using System;
using System.Collections.Generic;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using LudusaviWrap;
using Xunit;

namespace Spool.Tests;

// ── Test doubles ──────────────────────────────────────────────────────────────

class FakeDialogService : IDialogService
{
    public bool ConfirmResult { get; set; } = false;
    public List<(string Title, string Message)> Errors   { get; } = new();
    public List<(string Title, string Message)> Warnings { get; } = new();
    public List<(string Title, string Message)> Infos    { get; } = new();
    public List<(string Title, string Message)> Confirms { get; } = new();

    public void ShowError(string title, string message)   => Errors.Add((title, message));
    public void ShowWarning(string title, string message) => Warnings.Add((title, message));
    public void ShowInfo(string title, string message)    => Infos.Add((title, message));
    public bool Confirm(string title, string message)
    {
        Confirms.Add((title, message));
        return ConfirmResult;
    }
}

class FakeLockClient : IPlayStateLockClient
{
    public LockStatusResponse?    CheckLockResult    { get; set; }
    public LatestBackupResponse?  LatestBackupResult { get; set; }
    public AcquireResult          AcquireLockResult  { get; set; } = AcquireResult.Acquired;

    public Task<LockStatusResponse?>   CheckLockAsync(string _)           => Task.FromResult(CheckLockResult);
    public Task<LatestBackupResponse?> GetLatestBackupEventAsync(string _) => Task.FromResult(LatestBackupResult);
    public Task<AcquireResult>         AcquireLockAsync(string _)          => Task.FromResult(AcquireLockResult);
    public Task ReleaseLockAsync(string _)                                 => Task.CompletedTask;
    public Task StartHeartbeatLoopAsync(string _, CancellationToken __)    => Task.CompletedTask;
    public Task RecordRestoreAsync(string _)                               => Task.CompletedTask;
    public Task RecordBackupAsync(string _)                                => Task.CompletedTask;
    public Task UpdateLastPlayedRecordAsync(string _, DateTime __)         => Task.CompletedTask;
    public Task AddPlaytimeDeltaAsync(string _, int __)                    => Task.CompletedTask;
}

class FakeRunWorkflow : RunWorkflow
{
    private readonly Queue<ProcessResult> _processResults;

    public bool GameLaunched    { get; private set; }
    public int  ProcessCallCount { get; private set; }

    public FakeRunWorkflow(
        string gameName,
        string gameExe,
        GameEntry?            entry              = null,
        GameLibrary?          library            = null,
        Config?               config             = null,
        IDialogService?       dialogs            = null,
        IPlayStateLockClient? lockClientOverride = null,
        IEnumerable<ProcessResult>? processResults = null)
        : base(gameName, gameExe, entry, library, config, dialogs, lockClientOverride)
    {
        _processResults = new Queue<ProcessResult>(processResults ?? Array.Empty<ProcessResult>());
    }

    protected override Task<ProcessResult> RunProcessAsync(string filename, string arguments)
    {
        ProcessCallCount++;
        return Task.FromResult(_processResults.Count > 0
            ? _processResults.Dequeue()
            : new ProcessResult { ExitCode = 0, Output = "{}" });
    }

    protected override Task RunGameAsync(string exePath)
    {
        GameLaunched = true;
        return Task.CompletedTask;
    }
}

// ── Test fixture ──────────────────────────────────────────────────────────────

public class RunWorkflowTests : IDisposable
{
    // A real file on disk so IsLudusaviOk and File.Exists(_gameExe) both pass.
    private static readonly string FakeLudusaviPath =
        Path.Combine(Path.GetTempPath(), "spool_test_fake_ludusavi.exe");

    private static readonly string FakeGameExePath =
        Path.Combine(Path.GetTempPath(), "spool_test_fake_game.exe");

    private readonly string _tempLibDir;

    static RunWorkflowTests()
    {
        File.WriteAllBytes(FakeLudusaviPath, Array.Empty<byte>());
        File.WriteAllBytes(FakeGameExePath,  Array.Empty<byte>());
    }

    public RunWorkflowTests()
    {
        _tempLibDir = Path.Combine(Path.GetTempPath(), "spool_wf_test_" + Guid.NewGuid());
        Directory.CreateDirectory(_tempLibDir);
    }

    public void Dispose()
    {
        try { Directory.Delete(_tempLibDir, recursive: true); } catch { }
    }

    private Config MakeConfig(bool syncEnabled = false, string? syncUrl = null)
    {
        var cfg = new Config();
        cfg.Data.LudusaviPath      = FakeLudusaviPath;
        cfg.Data.SyncServerEnabled = syncEnabled;
        cfg.Data.SyncServerUrl     = syncUrl ?? "";
        return cfg;
    }

    private GameLibrary MakeLibrary() => new GameLibrary(_tempLibDir);

    // ── Helpers to build canned ProcessResults ─────────────────────────────────

    private static ProcessResult RestoreSuccess() => new()
    {
        ExitCode = 0,
        Output   = """{"errors":{},"overall":{"totalGames":1}}""",
    };

    private static ProcessResult RestoreUnknownGame(string name = "My Game") => new()
    {
        ExitCode = 0,
        Output   = "{\"errors\":{\"unknownGames\":[\"" + name + "\"]},\"overall\":{\"totalGames\":0}}",
    };

    private static ProcessResult RestoreCloudConflict() => new()
    {
        ExitCode = 0,
        Output   = """{"errors":{"cloudConflict":{}},"overall":{"totalGames":0}}""",
    };

    private static ProcessResult RestoreCloudSyncFailed() => new()
    {
        ExitCode = 0,
        Output   = """{"errors":{"cloudSyncFailed":{}},"overall":{"totalGames":1}}""",
    };

    private static ProcessResult RestoreFailure() => new()
    {
        ExitCode = 1,
        Output   = """{"errors":{},"overall":{"totalGames":1}}""",
        Error    = "restore failed",
    };

    private static ProcessResult BackupSuccess() => new()
    {
        ExitCode = 0,
        Output   = """{"errors":{},"overall":{"totalGames":1}}""",
    };

    private static ProcessResult BackupUnknownGame(string name = "My Game") => new()
    {
        ExitCode = 0,
        Output   = "{\"errors\":{\"unknownGames\":[\"" + name + "\"]},\"overall\":{\"totalGames\":0}}",
    };

    // ── Phase 2: Restore ───────────────────────────────────────────────────────

    [Fact]
    public async Task Restore_CloudConflict_AbortsAndCallsConfirm()
    {
        var dialogs = new FakeDialogService { ConfirmResult = false };
        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:  MakeConfig(),
            library: MakeLibrary(),
            dialogs: dialogs,
            processResults: new[] { RestoreCloudConflict() });

        await wf.ExecuteAsync();

        Assert.Contains(dialogs.Confirms, c => c.Title == "Cloud Sync Conflict");
        Assert.False(wf.GameLaunched, "game should not launch after cloud conflict");
    }

    [Fact]
    public async Task Restore_CloudSyncFailed_ContinuesAndLaunchesGame()
    {
        var dialogs = new FakeDialogService();
        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:  MakeConfig(),
            library: MakeLibrary(),
            dialogs: dialogs,
            processResults: new[] { RestoreCloudSyncFailed(), BackupSuccess() });

        await wf.ExecuteAsync();

        Assert.Empty(dialogs.Errors);
        Assert.True(wf.GameLaunched, "game should launch despite cloud sync failure");
    }

    [Fact]
    public async Task Restore_UnknownGame_ContinuesWithoutError()
    {
        var dialogs = new FakeDialogService();
        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:  MakeConfig(),
            library: MakeLibrary(),
            dialogs: dialogs,
            processResults: new[] { RestoreUnknownGame(), BackupUnknownGame() });

        await wf.ExecuteAsync();

        Assert.Empty(dialogs.Errors);
        Assert.True(wf.GameLaunched, "game should launch when game is unknown to ludusavi");
    }

    [Fact]
    public async Task Restore_NonZeroExitWithSaves_AbortsAndShowsError()
    {
        var dialogs = new FakeDialogService();
        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:  MakeConfig(),
            library: MakeLibrary(),
            dialogs: dialogs,
            processResults: new[] { RestoreFailure() });

        await wf.ExecuteAsync();

        Assert.NotEmpty(dialogs.Errors);
        Assert.False(wf.GameLaunched, "game should not launch after restore failure");
    }

    [Fact]
    public async Task Restore_Success_LaunchesGame()
    {
        var dialogs = new FakeDialogService();
        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:  MakeConfig(),
            library: MakeLibrary(),
            dialogs: dialogs,
            processResults: new[] { RestoreSuccess(), BackupSuccess() });

        await wf.ExecuteAsync();

        Assert.Empty(dialogs.Errors);
        Assert.True(wf.GameLaunched);
    }

    // ── Phase 1: Lock check ────────────────────────────────────────────────────

    [Fact]
    public async Task StaleLock_ShowsInfoNotConfirm_AndProceeds()
    {
        var dialogs = new FakeDialogService();
        var lock_   = new FakeLockClient
        {
            CheckLockResult = new LockStatusResponse
            {
                Locked   = true,
                Stale    = true,
                DeviceId = "other-device",
                DeviceName = "OtherPC",
            },
            LatestBackupResult = new LatestBackupResponse
            {
                Found      = true,
                OccurredAt = "2024-01-01T12:00:00Z",
                DeviceName = "OtherPC",
            },
            AcquireLockResult = AcquireResult.Acquired,
        };

        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:             MakeConfig(),
            library:            MakeLibrary(),
            dialogs:            dialogs,
            lockClientOverride: lock_,
            processResults:     new[] { RestoreSuccess(), BackupSuccess() });

        await wf.ExecuteAsync();

        Assert.Contains(dialogs.Infos, i => i.Title == "Stale Lock Detected");
        Assert.DoesNotContain(dialogs.Confirms, c => c.Title == "Game Already Running");
        Assert.True(wf.GameLaunched, "workflow should proceed past stale lock");
    }

    [Fact]
    public async Task NonStaleLock_UserDenies_AbortsBeforeRestore()
    {
        var dialogs = new FakeDialogService { ConfirmResult = false };
        var lock_   = new FakeLockClient
        {
            CheckLockResult = new LockStatusResponse
            {
                Locked   = true,
                Stale    = false,
                DeviceId = "other-device",
                DeviceName = "GameStation",
            },
        };

        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:             MakeConfig(),
            library:            MakeLibrary(),
            dialogs:            dialogs,
            lockClientOverride: lock_,
            processResults:     new[] { RestoreSuccess(), BackupSuccess() });

        await wf.ExecuteAsync();

        Assert.Contains(dialogs.Confirms, c => c.Title == "Game Already Running");
        Assert.Equal(0, wf.ProcessCallCount);   // restore never called
        Assert.False(wf.GameLaunched);
    }

    [Fact]
    public async Task NonStaleLock_UserConfirms_ContinuesWorkflow()
    {
        var dialogs = new FakeDialogService { ConfirmResult = true };
        var lock_   = new FakeLockClient
        {
            CheckLockResult = new LockStatusResponse
            {
                Locked   = true,
                Stale    = false,
                DeviceId = "other-device",
                DeviceName = "GameStation",
            },
            AcquireLockResult = AcquireResult.Acquired,
        };

        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:             MakeConfig(),
            library:            MakeLibrary(),
            dialogs:            dialogs,
            lockClientOverride: lock_,
            processResults:     new[] { RestoreSuccess(), BackupSuccess() });

        await wf.ExecuteAsync();

        Assert.True(wf.GameLaunched, "workflow should proceed when user confirms");
    }

    // ── LudusaviOk guard ──────────────────────────────────────────────────────

    [Fact]
    public async Task LudusaviNotConfigured_ShowsErrorAndAborts()
    {
        var dialogs = new FakeDialogService();
        var cfg     = MakeConfig();
        cfg.Data.LudusaviPath = "";   // not configured

        var wf = new FakeRunWorkflow("My Game", FakeGameExePath,
            config:  cfg,
            library: MakeLibrary(),
            dialogs: dialogs);

        await wf.ExecuteAsync();

        Assert.NotEmpty(dialogs.Errors);
        Assert.Equal(0, wf.ProcessCallCount);
        Assert.False(wf.GameLaunched);
    }
}
