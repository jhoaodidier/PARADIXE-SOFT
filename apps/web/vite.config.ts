import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";

const apiProxyTarget =
  process.env.VITE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";

export default defineConfig({
  build: {
    target: "esnext",
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-react": ["react", "react-dom"],
          "vendor-maplibre": ["maplibre-gl"],
          "vendor-pmtiles": ["pmtiles"],
        },
      },
    },
  },
  server: {
    proxy: {
      "/api": apiProxyTarget,
      "/tiles": {
        target: "http://127.0.0.1:9000",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/tiles/, "/paradixe-tiles"),
      },
      "/evidence": {
        target: "http://127.0.0.1:9000",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/evidence/, "/paradixe-evidence"),
      },
    },
  },
  preview: {
    proxy: {
      "/api": apiProxyTarget,
      "/tiles": {
        target: "http://127.0.0.1:9000",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/tiles/, "/paradixe-tiles"),
      },
      "/evidence": {
        target: "http://127.0.0.1:9000",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/evidence/, "/paradixe-evidence"),
      },
    },
  },
  plugins: [
    react(),
    VitePWA({
      registerType: "autoUpdate",
      includeAssets: ["icon.svg"],
      manifest: {
        name: "PARADIXE SOFT — Resiliencia Vial",
        short_name: "PARADIXE SOFT",
        description:
          "Plataforma Offline-First de resiliencia vial y logistica critica (TakumiSoft).",
        theme_color: "#0b4f6c",
        background_color: "#0f172a",
        display: "standalone",
        lang: "es",
        icons: [
          {
            src: "icon.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "any",
          },
        ],
      },
      workbox: {
        globPatterns: ["**/*.{js,css,html,svg,png,woff2}"],
        runtimeCaching: [
          {
            urlPattern: /\.pmtiles$/i,
            handler: "CacheFirst",
            options: {
              cacheName: "paradixe-pmtiles-cache",
              expiration: {
                maxEntries: 8,
                maxAgeSeconds: 60 * 60 * 24 * 14,
              },
            },
          },
        ],
      },
    }),
  ],
});
