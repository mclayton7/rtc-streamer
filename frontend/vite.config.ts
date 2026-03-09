import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import dts from 'vite-plugin-dts';

const isLib = process.env['BUILD_MODE'] === 'lib';

export default defineConfig({
  plugins: [
    react(),
    ...(isLib
      ? [dts({ include: ['src'], exclude: ['src/main.tsx', 'src/App.tsx'], rollupTypes: true })]
      : []),
  ],
  server: {
    port: 3000,
    proxy: {
      '/signal': {
        target: 'ws://localhost:8888',
        ws: true,
        changeOrigin: true,
      },
      '/api': {
        target: 'http://localhost:8888',
        changeOrigin: true,
      },
    },
  },
  build: isLib
    ? {
        lib: {
          entry: 'src/index.ts',
          name: 'RtcStreamer',
          formats: ['es', 'cjs'],
          fileName: (format) => `rtc-streamer.${format}.js`,
        },
        rollupOptions: {
          external: ['react', 'react-dom', 'leaflet'],
          output: {
            globals: {
              react: 'React',
              'react-dom': 'ReactDOM',
              leaflet: 'L',
            },
          },
        },
        outDir: 'dist',
      }
    : {
        outDir: '../static',
        emptyOutDir: true,
      },
});
