using LudusaviWrap;
using Xunit;

namespace Spool.Tests;

public class LauncherGeneratorTests
{
    [Theory]
    [InlineData("My Game",         "My Game")]
    [InlineData("Game",            "Game")]
    [InlineData("My:Game",         "My_Game")]
    [InlineData("My/Game",         "My_Game")]
    [InlineData("My\\Game",        "My_Game")]
    [InlineData("My<Game>",        "My_Game_")]
    [InlineData("My*Game",         "My_Game")]
    [InlineData("My?Game",         "My_Game")]
    [InlineData("My\"Game",        "My_Game")]
    [InlineData("My|Game",         "My_Game")]
    [InlineData("Game.",           "Game_")]
    [InlineData("Game...",         "Game_")]
    [InlineData("Game.exe",        "Game.exe")]
    [InlineData("Game.test.exe",   "Game.test.exe")]
    [InlineData("Elden Ring™", "Elden Ring")]   // ™ stripped
    [InlineData("中文",    "Game")]            // all non-ASCII → fallback
    [InlineData("",                "Game")]
    [InlineData("   ",             "Game")]
    [InlineData("  Game  ",        "Game")]
    [InlineData("My  Game",        "My Game")]
    [InlineData("ABCéDEF",    "ABCDEF")]          // é stripped, rest kept
    public void MakeSafeFilename(string input, string expected)
        => Assert.Equal(expected, LauncherGenerator.MakeSafeFilename(input));
}
