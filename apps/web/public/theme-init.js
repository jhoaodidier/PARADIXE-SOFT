try {
  var savedScheme = localStorage.getItem("color-scheme");
  var meta = document.querySelector('meta[name="color-scheme"]');
  if (meta) {
    if (savedScheme === "dark" || savedScheme === "light") {
      meta.content = savedScheme;
    } else if (savedScheme === "system") {
      meta.content = "light dark";
    }
  }
} catch (e) {}
