import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useI18n } from "../i18n";

const LATEST_RELEASE = "https://api.github.com/repos/PoUpA/totalKONTROL/releases/latest";
const STORAGE_KEY = "totalkontrol.updates";
const CHECK_EVERY_MS = 24 * 60 * 60 * 1000;

type Stored = { never?: boolean; checkedAt?: number; skipped?: string };

function load(): Stored {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
  } catch {
    return {};
  }
}

function store(value: Stored) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // Without storage, the check simply runs again next time.
  }
}

/** "0.1.10" > "0.1.9". */
export function isNewer(candidate: string, current: string): boolean {
  const parts = (v: string) =>
    v
      .replace(/^v/, "")
      .split(".")
      .map((n) => Number.parseInt(n, 10) || 0);
  const [a, b] = [parts(candidate), parts(current)];
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return false;
}

/** Tells, at most once a day, when a newer release is out on GitHub. */
export function UpdateNotice() {
  const { t } = useI18n();
  const [available, setAvailable] = useState<string | null>(null);

  useEffect(() => {
    const stored = load();
    if (stored.never || (stored.checkedAt && Date.now() - stored.checkedAt < CHECK_EVERY_MS)) return;
    (async () => {
      const [current, response] = await Promise.all([getVersion(), fetch(LATEST_RELEASE)]);
      if (!response.ok) return;
      const latest: string = (await response.json()).tag_name ?? "";
      store({ ...load(), checkedAt: Date.now() });
      if (isNewer(latest, current) && latest !== stored.skipped) setAvailable(latest);
    })().catch(() => {
      // Offline or rate-limited: nothing to tell.
    });
  }, []);

  if (!available) return null;
  return (
    <div className="banner" role="status">
      {t("update.available", { version: available.replace(/^v/, "") })}
      <div className="actions">
        <button className="primary" onClick={() => invoke("open_releases")}>
          {t("update.download")}
        </button>
        <button
          onClick={() => {
            store({ ...load(), skipped: available });
            setAvailable(null);
          }}
        >
          {t("update.later")}
        </button>
        <button
          onClick={() => {
            store({ ...load(), never: true });
            setAvailable(null);
          }}
        >
          {t("update.never")}
        </button>
      </div>
    </div>
  );
}
