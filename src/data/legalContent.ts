import appPackage from "../../package.json";
import licenseDirectoryReadme from "../../LICENSE/README.md?raw";
import gplLicenseText from "../../LICENSE/LICENSES/GPL-3.0.txt?raw";
import ffmpegGplText from "../../LICENSE/LICENSES/FFMPEG-GPL.txt?raw";
import otherLicensesText from "../../LICENSE/LICENSES/OTHER-LICENSES.txt?raw";
import thirdPartyNoticesText from "../../LICENSE/THIRD-PARTY-NOTICES.txt?raw";
import rootReadmeText from "../../README.md?raw";
import cargoManifestText from "../../src-tauri/Cargo.toml?raw";

export interface LegalDocument {
  id: string;
  title: string;
  fileName: string;
  filePath: string;
  content: string;
}

export interface LegalSummary {
  appName: string;
  appVersion: string;
  author: string;
  publisher: string;
  repository: string;
  homepage: string;
  license: string;
  copyright: string;
}

function extractReadmeLine(source: string, label: string): string | null {
  const line = source
    .split(/\r?\n/)
    .find((entry) => entry.startsWith(`${label}:`));

  if (!line) return null;
  return line.slice(label.length + 1).trim() || null;
}

function extractTomlValue(source: string, key: string): string | null {
  const line = source
    .split(/\r?\n/)
    .find((entry) => entry.trimStart().startsWith(`${key} =`));

  if (!line) return null;
  const match = line.match(/^\s*[^=]+\s*=\s*"([^"]+)"\s*$/);
  return match?.[1] ?? null;
}

function resolveRepositoryUrl(): string {
  const repo = appPackage.repository;
  if (typeof repo === "string") return repo;
  if (repo && typeof repo === "object" && "url" in repo) {
    return String(repo.url ?? "");
  }
  return "";
}

export const legalSummary: LegalSummary = {
  appName: appPackage.name,
  appVersion: appPackage.version,
  author:
    typeof appPackage.author === "string"
      ? appPackage.author
      : "Vishwas Sharma",
  publisher:
    extractReadmeLine(rootReadmeText, "Publisher") ?? "Software From Vish",
  repository:
    extractReadmeLine(rootReadmeText, "Repository") ||
    resolveRepositoryUrl() ||
    "n/a",
  homepage: appPackage.homepage ?? (resolveRepositoryUrl() || "n/a"),
  license: extractTomlValue(cargoManifestText, "license") ?? "GPL-3.0-or-later",
  copyright: appPackage.copyright,
};

export const legalDocuments: LegalDocument[] = [
  {
    id: "license-directory-overview",
    title: "License Directory Overview",
    fileName: "README.md",
    filePath: "LICENSE/README.md",
    content: licenseDirectoryReadme,
  },
  {
    id: "application-license",
    title: "Application License",
    fileName: "GPL-3.0.txt",
    filePath: "LICENSE/LICENSES/GPL-3.0.txt",
    content: gplLicenseText,
  },
  {
    id: "third-party-notices",
    title: "Third-Party Notices",
    fileName: "THIRD-PARTY-NOTICES.txt",
    filePath: "LICENSE/THIRD-PARTY-NOTICES.txt",
    content: thirdPartyNoticesText,
  },
  {
    id: "bundled-runtime-components",
    title: "Bundled Runtime Components",
    fileName: "FFMPEG-GPL.txt",
    filePath: "LICENSE/LICENSES/FFMPEG-GPL.txt",
    content: ffmpegGplText,
  },
  {
    id: "additional-notices",
    title: "Additional Notices",
    fileName: "OTHER-LICENSES.txt",
    filePath: "LICENSE/LICENSES/OTHER-LICENSES.txt",
    content: otherLicensesText,
  },
];
