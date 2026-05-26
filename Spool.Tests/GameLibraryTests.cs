using System;
using System.IO;
using LudusaviWrap;
using Xunit;

namespace Spool.Tests;

public class GameLibraryTests : IDisposable
{
    private readonly string _tempDir;

    public GameLibraryTests()
    {
        _tempDir = Path.Combine(Path.GetTempPath(), "spool_lib_test_" + Guid.NewGuid());
        Directory.CreateDirectory(_tempDir);
    }

    public void Dispose()
    {
        try { Directory.Delete(_tempDir, recursive: true); } catch { }
    }

    private GameLibrary NewLibrary() => new GameLibrary(_tempDir);

    private static GameEntry MakeEntry(string name) => new GameEntry
    {
        GameName = name,
        ExePath  = $@"C:\Games\{name}\game.exe",
        SafeName = name,
    };

    // ── Empty state ────────────────────────────────────────────────────────────

    [Fact]
    public void NewLibrary_NoFile_EmptyEntries()
    {
        var lib = NewLibrary();
        Assert.Empty(lib.Entries);
    }

    // ── Add ────────────────────────────────────────────────────────────────────

    [Fact]
    public void Add_AppendsEntry()
    {
        var lib   = NewLibrary();
        var entry = MakeEntry("Portal 2");

        lib.Add(entry);

        Assert.Single(lib.Entries);
        Assert.Equal("Portal 2", lib.Entries[0].GameName);
    }

    [Fact]
    public void Add_PersistsToDisk()
    {
        var lib = NewLibrary();
        lib.Add(MakeEntry("Portal 2"));

        var reloaded = NewLibrary();
        Assert.Single(reloaded.Entries);
        Assert.Equal("Portal 2", reloaded.Entries[0].GameName);
    }

    [Fact]
    public void Add_MultipleEntries_AllPersist()
    {
        var lib = NewLibrary();
        lib.Add(MakeEntry("Portal 2"));
        lib.Add(MakeEntry("Half-Life 2"));
        lib.Add(MakeEntry("Left 4 Dead 2"));

        var reloaded = NewLibrary();
        Assert.Equal(3, reloaded.Entries.Count);
    }

    // ── Remove ─────────────────────────────────────────────────────────────────

    [Fact]
    public void Remove_ById_RemovesEntry()
    {
        var lib   = NewLibrary();
        var entry = MakeEntry("Portal 2");
        lib.Add(entry);

        lib.Remove(entry.Id);

        Assert.Empty(lib.Entries);
    }

    [Fact]
    public void Remove_ById_PersistsDeletion()
    {
        var lib   = NewLibrary();
        var entry = MakeEntry("Portal 2");
        lib.Add(entry);
        lib.Remove(entry.Id);

        var reloaded = NewLibrary();
        Assert.Empty(reloaded.Entries);
    }

    [Fact]
    public void Remove_UnknownId_NoOp()
    {
        var lib = NewLibrary();
        lib.Add(MakeEntry("Portal 2"));

        lib.Remove("nonexistent-id");

        Assert.Single(lib.Entries);
    }

    [Fact]
    public void Remove_LeavesOtherEntriesIntact()
    {
        var lib = NewLibrary();
        var a   = MakeEntry("Portal 2");
        var b   = MakeEntry("Half-Life 2");
        lib.Add(a);
        lib.Add(b);

        lib.Remove(a.Id);

        Assert.Single(lib.Entries);
        Assert.Equal("Half-Life 2", lib.Entries[0].GameName);
    }

    // ── Update ─────────────────────────────────────────────────────────────────

    [Fact]
    public void Update_ChangesFieldInPlace()
    {
        var lib   = NewLibrary();
        var entry = MakeEntry("Portal 2");
        lib.Add(entry);

        entry.PlaytimeMinutes = 90;
        lib.Update(entry);

        Assert.Equal(90, lib.Entries[0].PlaytimeMinutes);
    }

    [Fact]
    public void Update_PersistsChange()
    {
        var lib   = NewLibrary();
        var entry = MakeEntry("Portal 2");
        lib.Add(entry);

        entry.PlaytimeMinutes = 90;
        lib.Update(entry);

        var reloaded = NewLibrary();
        Assert.Equal(90, reloaded.Entries[0].PlaytimeMinutes);
    }

    [Fact]
    public void Update_UnknownId_NoOp()
    {
        var lib   = NewLibrary();
        var extra = MakeEntry("Ghost Game");
        lib.Update(extra);   // id not in library
        Assert.Empty(lib.Entries);
    }

    // ── FindByName ─────────────────────────────────────────────────────────────

    [Fact]
    public void FindByName_ExactMatch_ReturnsEntry()
    {
        var lib = NewLibrary();
        lib.Add(MakeEntry("Portal 2"));

        var result = lib.FindByName("Portal 2");

        Assert.NotNull(result);
        Assert.Equal("Portal 2", result!.GameName);
    }

    [Fact]
    public void FindByName_CaseInsensitive()
    {
        var lib = NewLibrary();
        lib.Add(MakeEntry("Portal 2"));

        Assert.NotNull(lib.FindByName("portal 2"));
        Assert.NotNull(lib.FindByName("PORTAL 2"));
    }

    [Fact]
    public void FindByName_Unknown_ReturnsNull()
    {
        var lib = NewLibrary();
        lib.Add(MakeEntry("Portal 2"));

        Assert.Null(lib.FindByName("Half-Life 3"));
    }

    // ── Reload persistence ─────────────────────────────────────────────────────

    [Fact]
    public void Reload_FullRoundtrip_AllFieldsSurvive()
    {
        var lib   = NewLibrary();
        var ts    = new DateTime(2024, 3, 10, 8, 0, 0, DateTimeKind.Utc);
        var entry = new GameEntry
        {
            GameName        = "Elden Ring",
            ExePath         = @"C:\Games\EldenRing\game.exe",
            SafeName        = "Elden Ring",
            PlaytimeMinutes = 350,
            LastPlayedAt    = ts,
            RunAsAdmin      = true,
            LanShared       = true,
        };
        lib.Add(entry);

        var reloaded = NewLibrary();
        var r = reloaded.Entries[0];

        Assert.Equal("Elden Ring",            r.GameName);
        Assert.Equal(@"C:\Games\EldenRing\game.exe", r.ExePath);
        Assert.Equal(350,                     r.PlaytimeMinutes);
        Assert.Equal(ts,                      r.LastPlayedAt);
        Assert.True(r.RunAsAdmin);
        Assert.True(r.LanShared);
    }
}
