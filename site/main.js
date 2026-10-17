// Highlights the download that matches the visitor's computer. Nothing is stored or sent; without this script both
// downloads simply show side by side.
(function () {
  var ua = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || navigator.userAgent || "";
  var os = /mac/i.test(ua) && !/iphone|ipad/i.test(navigator.userAgent) ? "mac" : /win/i.test(ua) ? "windows" : null;
  if (!os) return;
  var hero = document.querySelector(".cta[data-downloads]");
  if (hero) {
    hero.querySelectorAll("[data-os]").forEach(function (a) {
      var mine = a.getAttribute("data-os") === os;
      a.classList.toggle("btn-strong", mine);
      if (mine) hero.insertBefore(a, hero.firstChild);
    });
  }
  var card = document.querySelector(".dl[data-os='" + os + "']");
  if (card) {
    card.classList.add("is-yours");
    card.parentNode.insertBefore(card, card.parentNode.firstChild);
  }
})();
