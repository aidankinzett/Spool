<script module lang="ts">
  import type { ComponentProps } from 'svelte';
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import LanDownloadProgress from './LanDownloadProgress.svelte';
  import type { DownloadProgress } from '$lib/types';

  const defaultDownload: DownloadProgress = {
    install_token: 'token123',
    source_device_id: 'device123',
    source_device_name: 'Steam Deck',
    source_game_id: 'game123',
    game_name: 'Hades',
    bytes_done: 450000000,
    bytes_total: 1000000000,
    current_file: 'Hades/Content/GameData.pkg',
    status: 'transferring',
    message: null,
    new_game_id: null,
    bytes_per_second: 12500000, // 12.5 MB/s
    cover_image_path: null,
  };

  const { Story } = defineMeta({
    title: 'Transfers/LanDownloadProgress',
    component: LanDownloadProgress,
    render: template,
    argTypes: {
      accent: { control: 'color' },
      barClass: { control: 'text' },
      metaClass: { control: 'text' },
    },
    args: {
      download: defaultDownload,
      accent: 'var(--color-spool)',
      barClass: 'h-1',
      metaClass: 'text-[9.5px]',
    },
  });
</script>

{#snippet template(args: ComponentProps<typeof LanDownloadProgress>)}
  <div style="width: 360px" class="p-4 bg-bg-1 rounded-lg">
    <LanDownloadProgress {...args} />
  </div>
{/snippet}

<Story name="Default" />

<Story name="Short Filename" args={{
  download: {
    ...defaultDownload,
    current_file: 'Hades.exe',
  }
}} />

<Story name="Medium Filename" args={{
  download: {
    ...defaultDownload,
    current_file: 'Engine/Binaries/Win64/UnrealEditor-Core.dll',
  }
}} />

<Story name="Long Filename" args={{
  download: {
    ...defaultDownload,
    current_file: 'Content/Paks/enterprise_features_heavy_graphics_and_texture_assets_pack_version_3_final_final_really_final.pak',
  }
}} />

<Story name="Extremely Long Filename" args={{
  download: {
    ...defaultDownload,
    current_file: 'SteamLibrary/steamapps/common/Cyberpunk 2077/archive/pc/content/basegame_4_gamedata_heavy_assets_compression_level_9_ultra_high_resolution_textures_and_audio_pack.archive',
  }
}} />

<Story name="No Filename (Empty)" args={{
  download: {
    ...defaultDownload,
    current_file: '',
  }
}} />

<Story name="Starting Phase" args={{
  download: {
    ...defaultDownload,
    status: 'starting',
    bytes_done: 0,
    bytes_total: 0,
    current_file: 'Requesting file manifest...',
    bytes_per_second: 0,
  }
}} />
