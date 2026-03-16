import { create } from "zustand";
import { persist } from "zustand/middleware";
import { isThemeMode, type ThemeMode } from "../constants/themes";
import {
  DEFAULT_COMPONENT_SCALE,
  DEFAULT_COMPONENT_SPACING,
  DEFAULT_FONT_SIZE,
  normalizeComponentScale,
  normalizeComponentSpacing,
  normalizeFontSize,
} from "../constants/preferences";

export type Language = "zh" | "en";

interface PreferencesState {
  theme: ThemeMode;
  language: Language;
  fontSize: number;
  componentScale: number;
  componentSpacing: number;
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setFontSize: (fontSize: number) => void;
  setComponentScale: (componentScale: number) => void;
  setComponentSpacing: (componentSpacing: number) => void;
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      theme: "light",
      language: "zh",
      fontSize: DEFAULT_FONT_SIZE,
      componentScale: DEFAULT_COMPONENT_SCALE,
      componentSpacing: DEFAULT_COMPONENT_SPACING,
      setTheme(theme) {
        set({ theme });
      },
      setLanguage(language) {
        set({ language });
      },
      setFontSize(fontSize) {
        set({ fontSize: normalizeFontSize(fontSize) });
      },
      setComponentScale(componentScale) {
        set({ componentScale: normalizeComponentScale(componentScale) });
      },
      setComponentSpacing(componentSpacing) {
        set({ componentSpacing: normalizeComponentSpacing(componentSpacing) });
      },
    }),
    {
      name: "nextchat-preferences",
      merge: (persistedState, currentState) => {
        const state = persistedState as Partial<PreferencesState> | undefined;
        return {
          ...currentState,
          ...state,
          theme:
            state?.theme && isThemeMode(state.theme) ? state.theme : currentState.theme,
          fontSize: normalizeFontSize(state?.fontSize ?? currentState.fontSize),
          componentScale: normalizeComponentScale(
            state?.componentScale ?? currentState.componentScale,
          ),
          componentSpacing: normalizeComponentSpacing(
            state?.componentSpacing ?? currentState.componentSpacing,
          ),
        };
      },
    },
  ),
);
