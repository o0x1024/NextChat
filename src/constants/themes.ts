export const daisyThemes = [
  "light",
  "dark",
  "cupcake",
  "bumblebee",
  "emerald",
  "corporate",
  "synthwave",
  "retro",
  "cyberpunk",
  "valentine",
  "halloween",
  "garden",
  "forest",
  "aqua",
  "lofi",
  "pastel",
  "fantasy",
  "wireframe",
  "black",
  "luxury",
  "dracula",
  "cmyk",
  "autumn",
  "business",
  "acid",
  "lemonade",
  "night",
  "coffee",
  "winter",
  "dim",
  "nord",
  "sunset",
  "caramellatte",
  "abyss",
  "silk",
] as const;

export type ThemeMode = (typeof daisyThemes)[number];

export function isThemeMode(value: string): value is ThemeMode {
  return (daisyThemes as readonly string[]).includes(value);
}

const themeLabels: Record<ThemeMode, string> = {
  light: "Light",
  dark: "Dark",
  cupcake: "Cupcake",
  bumblebee: "Bumblebee",
  emerald: "Emerald",
  corporate: "Corporate",
  synthwave: "Synthwave",
  retro: "Retro",
  cyberpunk: "Cyberpunk",
  valentine: "Valentine",
  halloween: "Halloween",
  garden: "Garden",
  forest: "Forest",
  aqua: "Aqua",
  lofi: "Lo-fi",
  pastel: "Pastel",
  fantasy: "Fantasy",
  wireframe: "Wireframe",
  black: "Black",
  luxury: "Luxury",
  dracula: "Dracula",
  cmyk: "CMYK",
  autumn: "Autumn",
  business: "Business",
  acid: "Acid",
  lemonade: "Lemonade",
  night: "Night",
  coffee: "Coffee",
  winter: "Winter",
  dim: "Dim",
  nord: "Nord",
  sunset: "Sunset",
  caramellatte: "Caramel Latte",
  abyss: "Abyss",
  silk: "Silk",
};

export function formatThemeName(theme: ThemeMode) {
  return themeLabels[theme];
}

export function applyThemeToDocument(theme: ThemeMode) {
  if (typeof document === "undefined") return;

  document.documentElement.setAttribute("data-theme", theme);
  document.body.setAttribute("data-theme", theme);
  document.getElementById("root")?.setAttribute("data-theme", theme);
}
