using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using LudusaviWrap;
using Xunit;

namespace Spool.Tests;

public class GameEntryTests
{
    // ── JSON roundtrip ─────────────────────────────────────────────────────────

    [Fact]
    public void Roundtrip_BasicFields()
    {
        var entry = new GameEntry
        {
            GameName   = "Portal 2",
            ExePath    = @"C:\Games\portal2.exe",
            SafeName   = "Portal 2",
            AddedAt    = new DateTime(2024, 1, 15, 12, 0, 0, DateTimeKind.Utc),
            PlaytimeMinutes = 120,
            RunAsAdmin = true,
            Description = "Valve puzzle game",
            Genres = new List<string> { "Puzzle", "FPS" },
        };

        string json = JsonSerializer.Serialize(new List<GameEntry> { entry },
            LibrarySourceGenerationContext.Default.ListGameEntry);
        var loaded = JsonSerializer.Deserialize(json,
            LibrarySourceGenerationContext.Default.ListGameEntry);

        Assert.NotNull(loaded);
        Assert.Single(loaded);
        var result = loaded[0];
        Assert.Equal("Portal 2",             result.GameName);
        Assert.Equal(@"C:\Games\portal2.exe", result.ExePath);
        Assert.Equal("Portal 2",             result.SafeName);
        Assert.Equal(120,                    result.PlaytimeMinutes);
        Assert.True(result.RunAsAdmin);
        Assert.Equal("Valve puzzle game",    result.Description);
        Assert.Equal(new List<string> { "Puzzle", "FPS" }, result.Genres);
    }

    [Fact]
    public void Roundtrip_NullableFields_Null()
    {
        var entry = new GameEntry { GameName = "TestGame" };

        string json = JsonSerializer.Serialize(new List<GameEntry> { entry },
            LibrarySourceGenerationContext.Default.ListGameEntry);
        var loaded = JsonSerializer.Deserialize(json,
            LibrarySourceGenerationContext.Default.ListGameEntry);

        Assert.NotNull(loaded);
        var result = loaded![0];
        Assert.Null(result.LastPlayedAt);
        Assert.Null(result.ReleaseDate);
        Assert.Null(result.LauncherExePath);
    }

    [Fact]
    public void Roundtrip_LastPlayedAt_Preserved()
    {
        var ts = new DateTime(2024, 6, 1, 10, 30, 0, DateTimeKind.Utc);
        var entry = new GameEntry { GameName = "TestGame", LastPlayedAt = ts };

        string json = JsonSerializer.Serialize(new List<GameEntry> { entry },
            LibrarySourceGenerationContext.Default.ListGameEntry);
        var loaded = JsonSerializer.Deserialize(json,
            LibrarySourceGenerationContext.Default.ListGameEntry)!;

        Assert.Equal(ts, loaded[0].LastPlayedAt);
    }

    // ── MigratePath logic ──────────────────────────────────────────────────────

    [Fact]
    public void MigratePath_NullPath_ReturnsNull()
    {
        var entry = new GameEntry { GameName = "X" };
        entry.CoverImagePath = null;
        Assert.Null(entry.CoverImagePath);
    }

    [Fact]
    public void MigratePath_NoLudusaviWrapSegment_Unchanged()
    {
        var entry = new GameEntry();
        entry.CoverImagePath = @"C:\Spool\covers\game.png";
        Assert.Equal(@"C:\Spool\covers\game.png", entry.CoverImagePath);
    }

    [Fact]
    public void MigratePath_LudusaviWrapSegment_MigratedFileExists_ReturnsMigrated()
    {
        string tempRoot = Path.Combine(Path.GetTempPath(), "spool_test_" + Guid.NewGuid());
        string oldPath  = Path.Combine(tempRoot, "ludusavi-wrap", "covers", "game.png");
        string newPath  = Path.Combine(tempRoot, "Spool", "covers", "game.png");
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(newPath)!);
            File.WriteAllText(newPath, "");

            var entry = new GameEntry();
            entry.CoverImagePath = oldPath;
            Assert.Equal(newPath, entry.CoverImagePath);
        }
        finally
        {
            try { Directory.Delete(tempRoot, recursive: true); } catch { }
        }
    }

    [Fact]
    public void MigratePath_LudusaviWrapSegment_MigratedFileMissing_ReturnsOriginal()
    {
        string oldPath = @"C:\fake\ludusavi-wrap\covers\game.png";
        var entry = new GameEntry();
        entry.CoverImagePath = oldPath;
        Assert.Equal(oldPath, entry.CoverImagePath);
    }

    [Fact]
    public void MigratePath_CaseInsensitive_Uppercase()
    {
        string tempRoot = Path.Combine(Path.GetTempPath(), "spool_test_" + Guid.NewGuid());
        string oldPath  = Path.Combine(tempRoot, "LUDUSAVI-WRAP", "covers", "game.png");
        string newPath  = Path.Combine(tempRoot, "Spool", "covers", "game.png");
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(newPath)!);
            File.WriteAllText(newPath, "");

            var entry = new GameEntry();
            entry.CoverImagePath = oldPath;
            Assert.Equal(newPath, entry.CoverImagePath);
        }
        finally
        {
            try { Directory.Delete(tempRoot, recursive: true); } catch { }
        }
    }

    [Fact]
    public void MigratePath_AppliesToHeroImagePath_Independently()
    {
        string tempRoot  = Path.Combine(Path.GetTempPath(), "spool_test_" + Guid.NewGuid());
        string oldHero   = Path.Combine(tempRoot, "ludusavi-wrap", "covers", "hero.png");
        string newHero   = Path.Combine(tempRoot, "Spool", "covers", "hero.png");
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(newHero)!);
            File.WriteAllText(newHero, "");

            var entry = new GameEntry();
            entry.HeroImagePath  = oldHero;
            entry.CoverImagePath = @"C:\no-migration-needed\cover.png";

            Assert.Equal(newHero, entry.HeroImagePath);
            Assert.Equal(@"C:\no-migration-needed\cover.png", entry.CoverImagePath);
        }
        finally
        {
            try { Directory.Delete(tempRoot, recursive: true); } catch { }
        }
    }
}
