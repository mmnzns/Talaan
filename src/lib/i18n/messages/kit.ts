import { area } from "../core";

export const kit = area({
  en: {
    editText: "Edit text",
    jumpTo: "Jump to {time}",
    errorLater: "{message}. Coming in a later build.",
    errorGone: "That meeting is gone.",
    errorNewer: "This meeting was saved by a newer version of Talaan.",
    errorGeneric: "Something went wrong. Your data is safe.",
  },
  fr: {
    editText: "Modifier le texte",
    jumpTo: "Aller à {time}",
    errorLater: "{message}. Disponible dans une prochaine version.",
    errorGone: "Cette réunion n’existe plus.",
    errorNewer: "Cette réunion a été enregistrée par une version plus récente de Talaan.",
    errorGeneric: "Une erreur s’est produite. Vos données sont en sécurité.",
  },
  es: {
    editText: "Editar texto",
    jumpTo: "Ir a {time}",
    errorLater: "{message}. Llegará en una versión posterior.",
    errorGone: "Esa reunión ya no existe.",
    errorNewer: "Esta reunión se guardó con una versión más reciente de Talaan.",
    errorGeneric: "Algo salió mal. Tus datos están a salvo.",
  },
});
