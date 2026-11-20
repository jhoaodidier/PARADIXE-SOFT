"use strict";

// Coordinates share the 700 × 400 projection of the illustrated terrain.
const routePath = "M84.78 214.26 L86.12 218.10 L87.51 222.82 L89.00 226.70 L90.63 227.90 L92.43 226.69 L94.44 224.25 L96.67 221.49 L99.12 218.87 L101.79 216.58 L104.65 214.71 L107.69 213.26 L110.85 212.19 L114.09 211.50 L117.37 211.26 L120.62 211.57 L123.79 212.50 L126.82 213.95 L129.68 215.58 L132.32 217.05 L134.70 218.13 L136.82 218.78 L138.65 219.07 L140.20 219.06 L141.50 218.80 L142.56 218.32 L143.43 218.31 L144.16 218.76 L144.80 219.13 L145.41 219.42 L146.04 219.64 L146.77 219.79 L147.65 219.81 L148.73 219.52 L150.04 218.66 L151.62 220.55 L153.50 223.04 L155.67 225.28 L158.14 226.11 L160.89 225.27 L163.88 224.08 L167.09 223.82 L170.46 224.63 L173.95 226.04 L177.50 227.41 L181.05 228.15 L184.56 228.20 L187.97 228.17 L191.24 228.42 L194.34 228.58 L197.25 228.23 L199.96 227.47 L202.47 226.66 L204.78 226.06 L206.93 225.67 L208.94 225.60 L210.86 226.58 L212.72 227.31 L214.57 228.08 L216.47 228.99 L218.46 229.80 L220.57 230.04 L222.85 229.36 L225.32 227.78 L228.00 225.51 L230.88 222.67 L233.96 219.12 L237.21 214.52 L240.62 208.60 L244.14 203.76 L247.71 207.64 L251.29 215.06 L254.81 220.52 L258.22 224.21 L261.46 226.49 L264.48 227.78 L267.23 228.44 L269.68 228.70 L271.80 228.69 L273.58 228.52 L275.01 228.40 L276.12 228.75 L276.92 230.13 L277.45 232.77 L277.77 235.89 L277.91 238.03 L277.95 238.50 L277.95 237.88 L277.97 237.09 L278.07 236.47 L278.31 235.99 L278.73 235.55 L279.38 235.13 L280.28 234.72 L281.45 234.26 L282.90 233.70 L284.61 232.97 L286.58 231.89 L288.76 229.84 L291.13 225.76 L293.64 218.52 L296.24 213.51 L298.89 210.93 L301.54 206.42 L304.15 201.05 L306.69 194.31 L309.12 186.50 L311.43 178.58 L313.60 171.34 L315.65 164.97 L317.59 159.23 L319.44 158.65 L321.24 159.76 L323.02 160.01 L324.84 159.46 L326.75 158.14 L328.78 156.07 L331.00 153.41 L333.44 150.47 L336.14 147.34 L339.12 143.85 L342.41 139.98 L345.99 135.18 L349.87 130.00 L354.02 134.81 L358.41 134.69 L362.99 134.74 L367.71 138.69 L372.51 137.02 L377.32 141.20 L382.07 158.27 L386.70 171.05 L391.15 182.58 L395.34 192.40 L399.25 198.39 L402.82 202.33 L406.04 205.84 L408.88 207.45 L411.34 211.05 L413.45 215.15 L415.21 218.01 L416.67 220.08 L417.87 221.60 L418.86 222.71 L419.69 223.51 L420.42 224.05 L421.10 224.41 L421.79 224.70 L422.53 225.09 L423.36 225.74 L424.30 226.76 L425.38 228.10 L426.59 229.58 L427.94 231.11 L429.40 232.75 L430.97 234.54 L432.59 236.34 L434.24 237.95 L435.87 239.26 L437.45 240.15 L438.93 240.52 L440.28 240.41 L441.47 240.02 L442.48 239.58 L443.29 239.22 L443.92 239.00 L444.37 238.94 L444.66 239.06 L444.84 239.34 L444.94 239.74 L445.02 240.23 L445.14 240.75 L445.36 241.25 L445.74 241.67 L446.34 241.96 L447.22 242.03 L448.44 241.77 L450.02 240.95 L452.01 238.89 L454.40 234.38 L457.22 235.47 L460.45 238.05 L464.06 236.16 L468.02 232.09 L472.29 226.06 L476.81 218.08 L481.51 207.52 L486.33 190.71 L491.21 186.41 L496.07 179.14 L500.87 171.43 L505.54 164.66 L510.04 157.99 L514.33 165.04 L518.38 168.31 L522.20 168.96 L525.77 170.96 L529.10 171.95 L532.23 169.54 L535.17 168.45 L537.97 174.21 L540.65 178.58 L543.28 181.93 L545.88 184.68 L548.49 187.15 L551.15 189.47 L553.88 191.44 L556.69 192.51 L559.60 191.95 L562.58 189.16 L565.64 194.80 L568.74 198.67 L571.85 199.71 L574.91 198.89 L577.90 197.42 L580.75 196.03 L583.41 194.79 L585.83 193.53 L587.98 192.24 L589.80 191.12 L591.28 190.45 L592.40 190.40 L593.15 191.02 L593.54 192.27 L593.60 194.07 L593.35 196.31 L592.84 198.90 L592.13 201.76 L591.28 204.80 L590.36 207.93";
const places = [
  { name: "Huánuco", x: 84.78, y: 214.26, lift: 50, width: 86 },
  { name: "Carpish", x: 325.78, y: 158.90, lift: 76, width: 82 },
  { name: "Tingo María", x: 590.36, y: 207.93, lift: 64, width: 112 }
];

function drawMap(container) {
  const blocked = container.dataset.map === "incident";
  const pins = places.map(({name, x, y, lift, width}, index) => {
    const alert = blocked && index === 1;
    const label = alert ? "Carpish · closed" : name;
    const size = alert ? 150 : width;
    return `<g transform="translate(${x} ${y})"><g class="map-label">
      <path d="M0 0v-${lift}" stroke="#354d38" stroke-width="1.3"/>
      ${alert ? '<circle class="incident-ring" r="28" fill="none" stroke="#ab4827" stroke-width="2"/>' : ""}
      <circle r="${alert ? 10 : 5.5}" fill="${alert ? "#ab4827" : "#f6f5e8"}" stroke="#8a482b" stroke-width="2.5"/>
      ${alert ? '<path d="m-4-4 8 8m0-8-8 8" stroke="white" stroke-width="2"/>' : ""}
      <g transform="translate(0 ${-lift})"><g class="place-name">
        <rect x="${-size/2}" y="-22" width="${size}" height="26" rx="3" fill="${alert ? "#a04423" : "#f7f6e9"}" stroke="${alert ? "#a04423" : "#9caa8f"}"/>
        <text y="-4" text-anchor="middle" font-family="Arial,sans-serif" font-size="15" font-weight="600" fill="${alert ? "#fff" : "#304732"}">${label}</text>
      </g></g>
    </g></g>`;
  }).join("");
  const routeId = "route-" + container.dataset.map;
  container.innerHTML = `<svg viewBox="0 15 700 390" role="img" aria-label="Schematic terrain along the road from Huánuco through Carpish to Tingo María${blocked ? ', with a simulated road closure at Carpish' : ''}.">
    <defs><path id="${routeId}" d="${routePath}" pathLength="1"/></defs>
    <image width="700" height="400" href="assets/relieve.webp"/>
    <g fill="none" stroke-linejoin="round" stroke-linecap="round">
      <use href="#${routeId}" stroke="#f9edcb" stroke-width="6" opacity=".85"/>
      <use href="#${routeId}" stroke="#be542b" stroke-width="3.5"/>
    </g>${pins}</svg>`;
}

// Each plate uses its own visual units. The text and final values stay readable throughout.
function buildCharts() {
  const strip = document.querySelector(".weight-strip");
  const colors = ["rain", "slope", "soil", "history"];
  const percentages = document.querySelectorAll(".weight-key dd");
  percentages.forEach((item, factor) => {
    const value = parseInt(item.textContent, 10);
    for (let i = 0; i < value / 10; i++) {
      const tile = document.createElement("i");
      tile.className = colors[factor];
      tile.style.setProperty("--order", strip.children.length);
      strip.append(tile);
    }
  });

  for (const row of document.querySelectorAll(".hour-cells")) {
    for (let hour = 1; hour <= 14; hour++) {
      const cell = document.createElement("i");
      if (row.hasAttribute("data-transfer") && hour > 2) {
        cell.className = "saved-hour";
        cell.style.setProperty("--order", 14 - hour);
      }
      row.append(cell);
    }
  }
}

class Presentation {
  constructor() {
    this.slides = Array.from(document.querySelectorAll(".slide"));
    this.dialog = document.querySelector("#contents");
    this.progress = document.querySelector("#progress");
    this.chapters = document.querySelector("#chapters");
    this.previous = document.querySelector("#previous");
    this.next = document.querySelector("#next");
    this.screenButton = document.querySelector("#fullscreen");
    this.current = -1;
    this.lastWheel = -Infinity;
    this.wheelDistance = 0;
    this.wheelConsumed = false;
    this.touch = null;

    for (const map of document.querySelectorAll("[data-map]")) drawMap(map);
    buildCharts();
    this.slides.forEach((slide, i) => {
      const name = slide.dataset.name;
      const number = this.number(i);
      slide.setAttribute("role", "group");
      slide.setAttribute("aria-roledescription", "slide");
      const dot = document.createElement("button");
      dot.title = number + " · " + name;
      dot.setAttribute("aria-label", "Go to " + dot.title);
      dot.addEventListener("click", () => this.show(i));
      this.progress.append(dot);
      const entry = document.createElement("button");
      entry.innerHTML = "<span>" + number + "</span>" + name;
      entry.addEventListener("click", () => {
        this.dialog.close();
        this.show(i);
      });
      this.chapters.append(entry);
    });
    this.bindControls();
    this.show(this.readHash(), false);
  }

  number(i) { return String(i + 1).padStart(2, "0"); }

  readHash() {
    const match = location.hash.match(/^#(?:slide|diapositiva)-(\d+)$/);
    return match ? Math.max(0, Math.min(this.slides.length - 1, Number(match[1]) - 1)) : 0;
  }

  show(i, updateHash = true) {
    if (i === this.current || i < 0 || i >= this.slides.length) return;
    const restoreFocus = this.slides[this.current]?.contains(document.activeElement);
    this.current = i;
    this.slides.forEach((slide, j) => {
      slide.hidden = j !== i;
      slide.classList.toggle("active", j === i);
      for (const button of [this.progress.children[j], this.chapters.children[j]]) {
        if (j === i) button.setAttribute("aria-current", "step");
        else button.removeAttribute("aria-current");
      }
      this.progress.children[j].classList.toggle("visited", j < i);
    });
    this.previous.disabled = i === 0;
    this.next.disabled = i === this.slides.length - 1;
    const name = this.slides[i].dataset.name;
    document.querySelector("#page").textContent = this.number(i);
    document.querySelector("#page-name").textContent = name;
    document.querySelector("#announcement").textContent = "Slide " + (i + 1) + " of " + this.slides.length + ". " + name;
    if (updateHash) history.replaceState(null, "", "#slide-" + (i + 1));
    if (restoreFocus) this.progress.children[i].focus({preventScroll:true});
  }

  openIndex() {
    if (this.dialog.open) return;
    this.dialog.showModal();
    this.chapters.children[this.current].focus();
  }

  async fullscreen() {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await document.documentElement.requestFullscreen();
    } catch {
      document.querySelector("#announcement").textContent = "The browser blocked full-screen mode.";
      this.screenButton.title = "The browser blocked full-screen mode";
    }
  }

  bindControls() {
    this.previous.addEventListener("click", () => this.show(this.current - 1));
    this.next.addEventListener("click", () => this.show(this.current + 1));
    document.querySelector("#start").addEventListener("click", () => this.show(1));
    document.querySelector("#restart").addEventListener("click", () => this.show(0));
    document.querySelector("#open-index").addEventListener("click", () => this.openIndex());
    document.querySelector("#close-index").addEventListener("click", () => this.dialog.close());
    this.dialog.addEventListener("click", e => {
      if (e.target !== this.dialog) return;
      const r = this.dialog.getBoundingClientRect();
      if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) this.dialog.close();
    });
    this.screenButton.hidden = !document.fullscreenEnabled;
    this.screenButton.addEventListener("click", () => this.fullscreen());
    document.addEventListener("fullscreenchange", () => {
      const active = Boolean(document.fullscreenElement);
      const label = active ? "Exit full screen" : "Full screen";
      this.screenButton.querySelector("span").textContent = label;
      this.screenButton.setAttribute("aria-label", label);
      this.screenButton.title = label + " (F)";
      this.screenButton.setAttribute("aria-pressed", String(active));
    });
    window.addEventListener("hashchange", () => this.show(this.readHash(), false));
    document.addEventListener("keydown", e => {
      if (this.dialog.open) {
        if (e.key === "Escape") {
          e.preventDefault();
          this.dialog.close();
        }
        return;
      }
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      switch (e.key.toLowerCase()) {
        case " ":
          if (e.target.closest("button")) return;
          this.show(this.current + 1); break;
        case "arrowright": case "arrowdown": case "pagedown": this.show(this.current + 1); break;
        case "arrowleft": case "arrowup": case "pageup": this.show(this.current - 1); break;
        case "home": this.show(0); break;
        case "end": this.show(this.slides.length - 1); break;
        case "i": this.openIndex(); break;
        case "f": if (!this.screenButton.hidden) this.fullscreen(); break;
        default: return;
      }
      e.preventDefault();
    });
    // Consume each touchpad burst once so inertia cannot skip several slides.
    window.addEventListener("wheel", e => {
      if (this.dialog.open || e.ctrlKey) return;
      const now = performance.now();
      if (now - this.lastWheel > 190) {
        this.wheelDistance = 0;
        this.wheelConsumed = false;
      }
      this.lastWheel = now;
      if (this.wheelConsumed) return;
      const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      this.wheelDistance += delta * (e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? innerHeight : 1);
      if (Math.abs(this.wheelDistance) >= 45) {
        this.wheelConsumed = true;
        this.show(this.current + Math.sign(this.wheelDistance));
      }
    }, {passive:true});
    const deck = document.querySelector("#deck");
    deck.addEventListener("touchstart", e => {
      this.touch = e.touches.length === 1 ? e.touches[0] : null;
    }, {passive:true});
    deck.addEventListener("touchcancel", () => { this.touch = null; });
    deck.addEventListener("touchend", e => {
      if (!this.touch) return;
      const dx = e.changedTouches[0].clientX - this.touch.clientX;
      const dy = e.changedTouches[0].clientY - this.touch.clientY;
      this.touch = null;
      if (Math.abs(dx) > 44 && Math.abs(dx) > Math.abs(dy) * 1.3) this.show(this.current - Math.sign(dx));
    }, {passive:true});
  }
}

new Presentation();
