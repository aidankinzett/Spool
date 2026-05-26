using System;
using LudusaviWrap;

namespace Spool.Tests;

/// <summary>
/// Fluent builder for <see cref="GameEntry"/> test fixtures.
/// Sensible defaults for every field — override only what matters for the test.
/// </summary>
internal sealed class GameEntryBuilder
{
    private string   _gameName        = "Test Game";
    private string   _exePath         = @"C:\Games\TestGame\game.exe";
    private string   _safeName        = "Test Game";
    private int      _playtimeMinutes;
    private DateTime? _lastPlayedAt;
    private bool     _runAsAdmin;
    private bool     _lanShared;

    public GameEntryBuilder WithName(string name)
    {
        _gameName = name;
        _safeName = name;
        return this;
    }

    public GameEntryBuilder WithExePath(string path)         { _exePath          = path;    return this; }
    public GameEntryBuilder WithPlaytime(int minutes)        { _playtimeMinutes  = minutes; return this; }
    public GameEntryBuilder WithLastPlayedAt(DateTime ts)    { _lastPlayedAt     = ts;      return this; }
    public GameEntryBuilder AsAdmin()                        { _runAsAdmin       = true;    return this; }
    public GameEntryBuilder SharedOnLan()                    { _lanShared        = true;    return this; }

    public GameEntry Build() => new()
    {
        GameName        = _gameName,
        ExePath         = _exePath,
        SafeName        = _safeName,
        PlaytimeMinutes = _playtimeMinutes,
        LastPlayedAt    = _lastPlayedAt,
        RunAsAdmin      = _runAsAdmin,
        LanShared       = _lanShared,
    };
}
