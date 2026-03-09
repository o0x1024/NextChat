import { create } from "zustand";
import { persist } from "zustand/middleware";
import { isThemeMode, type ThemeMode } from "../constants/themes";

export type Language = "zh" | "en";

interface PreferencesState {
  theme: ThemeMode;
  language: Language;
  fontSize: number;
  componentSpacing: number;
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setFontSize: (fontSize: number) => void;
  setComponentSpacing: (componentSpacing: number) => void;
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      theme: "light",
      language: "zh",
      fontSize: 14,
      componentSpacing: 4,
      setTheme(theme) {
        set({ theme });
      },
      setLanguage(language) {
        set({ language });
      },
      setFontSize(fontSize) {
        set({ fontSize });
      },
      setComponentSpacing(componentSpacing) {
        set({ componentSpacing });
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
        };
      },
    },
  ),
);
