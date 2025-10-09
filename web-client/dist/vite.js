import wasm from 'vite-plugin-wasm'

/**
 * @typedef {Object} NimiqVitePluginOptions
 * @property {boolean} [topLevelAwait=false] - Enable top-level await support for older browsers
 * @property {boolean} [configureWorker=true] - Whether to configure worker plugins
 */

/**
 * Vite plugin for Nimiq blockchain integration
 * Configures WebAssembly support and optimizations required for @nimiq/core
 *
 * @param {NimiqVitePluginOptions} [options={}]
 * @returns {import('vite').Plugin[]}
 */
export default function nimiq(options = {}) {
  const { topLevelAwait = false, configureWorker = true } = options

  const plugins = [wasm()]

  if (topLevelAwait) {
    try {
      const topLevelAwaitPlugin = require('vite-plugin-top-level-await')
      const topLevelAwaitOptions = typeof topLevelAwait === 'object' ? topLevelAwait : undefined
      plugins.push(topLevelAwaitPlugin.default ? topLevelAwaitPlugin.default(topLevelAwaitOptions) : topLevelAwaitPlugin(topLevelAwaitOptions))
    }
    catch {
      console.warn('@nimiq/core vite plugin: vite-plugin-top-level-await is not installed. Skipping top-level await configuration.')
    }
  }

  plugins.push({
    name: '@nimiq/core:vite',
    config() {
      const config = {
        optimizeDeps: {
          exclude: ['@nimiq/core'],
        },
        build: {
          target: 'esnext',
          rollupOptions: {
            output: {
              format: 'es',
            },
          },
        },
      }

      if (configureWorker) {
        const workerPlugins = [wasm()]
        if (topLevelAwait) {
          try {
            const topLevelAwaitPlugin = require('vite-plugin-top-level-await')
            const topLevelAwaitOptions = typeof topLevelAwait === 'object' ? topLevelAwait : undefined
            workerPlugins.push(topLevelAwaitPlugin.default ? topLevelAwaitPlugin.default(topLevelAwaitOptions) : topLevelAwaitPlugin(topLevelAwaitOptions))
          }
          catch {
            // Already warned above
          }
        }

        config.worker = {
          format: 'es',
          plugins: () => workerPlugins,
        }
      }

      return config
    },
  })

  return plugins
}

export { nimiq }
