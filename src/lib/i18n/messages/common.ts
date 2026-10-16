import { area } from "../core";

export const common = area({
  en: {
    cancel: "Cancel",
    today: "today",
    yesterday: "yesterday",
    etaMin_one: "About {count} min left",
    etaMin_other: "About {count} min left",
    etaSoon: "Less than a minute left",
    etaAlmost: "Almost done",
  },
  fr: {
    cancel: "Annuler",
    today: "aujourd’hui",
    yesterday: "hier",
    etaMin_one: "Environ {count} min restante",
    etaMin_other: "Environ {count} min restantes",
    etaSoon: "Moins d’une minute restante",
    etaAlmost: "Presque terminé",
  },
  es: {
    cancel: "Cancelar",
    today: "hoy",
    yesterday: "ayer",
    etaMin_one: "Falta aproximadamente {count} min",
    etaMin_other: "Faltan aproximadamente {count} min",
    etaSoon: "Falta menos de un minuto",
    etaAlmost: "Ya casi termina",
  },
});
