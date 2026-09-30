/**
 * English (US) messages.
 *
 * Keys must mirror `zh-CN.ts` exactly — see that file for the conventions.
 */
export default {
  app: {
    name: "MC Link",
    tagline: "Minecraft LAN connector",
  },

  common: {
    next: "Next",
    back: "Back",
    finish: "Finish",
    cancel: "Cancel",
    confirm: "OK",
    loading: "Loading…",
    retry: "Retry",
    save: "Save",
    close: "Close",
  },

  oobe: {
    title: "Welcome to MC Link",
    subtitle: "Pick your language and region first — you can change them later in Settings.",
    languageLabel: "Language",
    regionLabel: "Region",
    regionHint: "Region is used to pick a closer service node.",
    detectedHint: "Pre-selected from your system settings; feel free to change it.",
    start: "Get started",
  },

  settings: {
    language: "Language",
    region: "Region",
  },

  /** Region code → display name. Unknown codes fall back to the raw code. */
  region: {
    CN: "Chinese Mainland",
    HK: "Hong Kong",
    TW: "Taiwan",
    JP: "Japan",
    KR: "South Korea",
    SG: "Singapore",
    US: "United States",
    GB: "United Kingdom",
    DE: "Germany",
    FR: "France",
    AU: "Australia",
    CA: "Canada",
    BR: "Brazil",
    OTHER: "Other / Not listed",
  },
};
