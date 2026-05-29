import fs from 'fs';
import path from 'path';
import https from 'https';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const LUDUSAVI_VER = '0.31.0';
const RCLONE_VER = '1.74.2';

const targets = {
  linux: [
    {
      name: 'ludusavi-x86_64-unknown-linux-gnu',
      url: `https://github.com/mtkennerly/ludusavi/releases/download/v${LUDUSAVI_VER}/ludusavi-v${LUDUSAVI_VER}-linux.tar.gz`,
      archiveType: 'tar.gz',
      extractFile: 'ludusavi'
    },
    {
      name: 'rclone-x86_64-unknown-linux-gnu',
      url: `https://github.com/rclone/rclone/releases/download/v${RCLONE_VER}/rclone-v${RCLONE_VER}-linux-amd64.zip`,
      archiveType: 'zip',
      extractFile: `rclone-v${RCLONE_VER}-linux-amd64/rclone`
    }
  ],
  win32: [
    {
      name: 'ludusavi-x86_64-pc-windows-msvc.exe',
      url: `https://github.com/mtkennerly/ludusavi/releases/download/v${LUDUSAVI_VER}/ludusavi-v${LUDUSAVI_VER}-win64.zip`,
      archiveType: 'zip',
      extractFile: 'ludusavi.exe'
    },
    {
      name: 'rclone-x86_64-pc-windows-msvc.exe',
      url: `https://github.com/rclone/rclone/releases/download/v${RCLONE_VER}/rclone-v${RCLONE_VER}-windows-amd64.zip`,
      archiveType: 'zip',
      extractFile: `rclone-v${RCLONE_VER}-windows-amd64/rclone.exe`
    }
  ]
};

const downloadFile = (url, dest) => {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    https.get(url, (response) => {
      if (response.statusCode === 302 || response.statusCode === 301) {
        downloadFile(response.headers.location, dest).then(resolve).catch(reject);
        return;
      }
      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download: Status Code ${response.statusCode}`));
        return;
      }
      response.pipe(file);
      file.on('finish', () => {
        file.close();
        resolve();
      });
    }).on('error', (err) => {
      fs.unlink(dest, () => {});
      reject(err);
    });
  });
};

const extractArchive = (archivePath, tempExtractDir, archiveType) => {
  if (process.platform === 'win32') {
    execSync(`powershell -NoProfile -Command "Expand-Archive -Path '${archivePath}' -DestinationPath '${tempExtractDir}' -Force"`, { stdio: 'inherit' });
  } else {
    if (archiveType === 'tar.gz') {
      execSync(`tar -xzf "${archivePath}" -C "${tempExtractDir}"`, { stdio: 'inherit' });
    } else if (archiveType === 'zip') {
      execSync(`unzip -o "${archivePath}" -d "${tempExtractDir}"`, { stdio: 'inherit' });
    }
  }
};

async function main() {
  const args = process.argv.slice(2);
  let platform = process.platform; // Default to host platform
  
  if (args.includes('--all')) {
    platform = 'all';
  } else if (args.includes('--platform')) {
    const idx = args.indexOf('--platform');
    if (idx !== -1 && args[idx + 1]) {
      platform = args[idx + 1];
    }
  }

  const platformsToDownload = platform === 'all' ? ['linux', 'win32'] : [platform];
  const binariesDir = path.join(__dirname, '../src-tauri/binaries');
  
  if (!fs.existsSync(binariesDir)) {
    fs.mkdirSync(binariesDir, { recursive: true });
  }

  const tempDir = path.join(__dirname, '../.temp_sidecars');
  if (!fs.existsSync(tempDir)) {
    fs.mkdirSync(tempDir, { recursive: true });
  }

  for (const plat of platformsToDownload) {
    const list = targets[plat];
    if (!list) {
      console.error(`Unknown/unsupported platform: ${plat}`);
      continue;
    }

    console.log(`Downloading sidecars for platform: ${plat}...`);
    for (const item of list) {
      const ext = item.archiveType === 'zip' ? '.zip' : '.tar.gz';
      const archivePath = path.join(tempDir, `${item.name}${ext}`);
      const extractTempDir = path.join(tempDir, item.name);
      
      if (!fs.existsSync(extractTempDir)) {
        fs.mkdirSync(extractTempDir, { recursive: true });
      }

      console.log(`Downloading ${item.url} -> ${archivePath}`);
      await downloadFile(item.url, archivePath);

      console.log(`Extracting ${archivePath} -> ${extractTempDir}`);
      extractArchive(archivePath, extractTempDir, item.archiveType);

      const srcFile = path.join(extractTempDir, item.extractFile);
      const destFile = path.join(binariesDir, item.name);

      console.log(`Copying ${srcFile} -> ${destFile}`);
      fs.rmSync(destFile, { force: true });
      fs.copyFileSync(srcFile, destFile);
      
      // On Unix-like systems, ensure the final binary is executable
      if (process.platform !== 'win32' && plat === 'linux') {
        fs.chmodSync(destFile, 0o755);
      }
    }
  }

  // Cleanup temp dir
  console.log('Cleaning up temporary directory...');
  fs.rmSync(tempDir, { recursive: true, force: true });
  console.log('Sidecar download complete!');
}

main().catch((err) => {
  console.error('Error downloading sidecars:', err);
  process.exit(1);
});
