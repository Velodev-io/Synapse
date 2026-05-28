<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Spotlight from './Spotlight.svelte';

  // --- State ---
  let isSpotlight = false;
  let settings = {
    providers: {
      perplexity: { enabled: true },
      chatgpt: { enabled: true },
      claude: { enabled: false },
      gemini: { enabled: true },
      deepseek: { enabled: false },
      kimi: { enabled: false }
    },
    restApiEnabled: false,
    fileReferenceEnabled: true
  };

  let activeTab = 'welcome';
  let mcpConfig = {};
  let loginStates = {
    perplexity: false,
    chatgpt: false,
    claude: false,
    gemini: false,
    deepseek: false,
    kimi: false
  };

  let toast = '';
  let showCookieModal = false;
  let cookieText = '';
  let cookieModalProvider = '';
  let cookieStatus = '';
  let cookieStatusClass = '';
  let cookieButtonText = '🔓 Login with Cookies';
  let cookieButtonDisabled = false;

  let cliInstalled = false;
  let cliInstallStatus = '';
  let cliButtonText = '⚡ Install CLI to PATH';
  let cliButtonDisabled = false;

  let configMethod = 'manual';

  // --- Analytics State ---
  let statsData = {
    uptime: '0s',
    totalRequests: 0,
    totalErrors: 0,
    providers: {} as Record<string, { calls: number; errors: number; avg_time_ms: number; min_time_ms: number; max_time_ms: number; last_call: string | null }>
  };
  let historyData = [] as Array<{ timestamp: string; model: string; query: string; response: string }>;
  let loadingAnalytics = false;
  let analyticsInterval: any = null;

  async function loadAnalytics() {
    loadingAnalytics = true;
    try {
      const statsRes = await fetch('http://127.0.0.1:3210/v1/stats');
      if (statsRes.ok) {
        statsData = await statsRes.json();
      }
      
      const historyRes = await fetch('http://127.0.0.1:3210/v1/history');
      if (historyRes.ok) {
        const histJson = await historyRes.json();
        historyData = histJson.history || [];
      }
    } catch (err) {
      console.error('Failed to load analytics', err);
    } finally {
      loadingAnalytics = false;
    }
  }

  function startAnalyticsPolling() {
    stopAnalyticsPolling();
    loadAnalytics();
    analyticsInterval = setInterval(loadAnalytics, 5000);
  }

  function stopAnalyticsPolling() {
    if (analyticsInterval) {
      clearInterval(analyticsInterval);
      analyticsInterval = null;
    }
  }

  function exportAsJSON() {
    if (historyData.length === 0) {
      showToast('No history to export');
      return;
    }
    const blob = new Blob([JSON.stringify(historyData, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `proxima_history_${new Date().toISOString().slice(0, 10)}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    showToast('Exported history as JSON');
  }

  function exportAsMarkdown() {
    if (historyData.length === 0) {
      showToast('No history to export');
      return;
    }
    let md = `# Proxima Chat History\nExported on: ${new Date().toLocaleString()}\n\n`;
    for (const item of historyData) {
      md += `## [${item.timestamp}] Model: ${item.model}\n\n`;
      md += `### Prompt:\n${item.query}\n\n`;
      md += `### Response:\n${item.response}\n\n`;
      md += `---\n\n`;
    }
    const blob = new Blob([md], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `proxima_history_${new Date().toISOString().slice(0, 10)}.md`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    showToast('Exported history as Markdown');
  }

  // --- Helper to show Toast ---
  function showToast(message: string) {
    toast = message;
    setTimeout(() => {
      if (toast === message) toast = '';
    }, 2500);
  }

  // --- IPC Interface Safe Calls ---
  const agentHub = (window as any).agentHub;

  onMount(async () => {
    isSpotlight = window.location.hash === '#spotlight';
    if (isSpotlight) return;

    if (!agentHub) {
      console.error('[Proxima] Electron preload API not found');
      return;
    }

    try {
      // Load settings
      const saved = await agentHub.getSettings();
      if (saved && saved.settings) {
        settings = saved.settings;
      }
      
      // Load MCP config
      mcpConfig = await agentHub.getMcpConfig();

      // Check CLI status
      cliInstalled = await agentHub.isCliInstalled();
      updateCliUI();

      // Check initial login status of all enabled providers
      checkAllStatus();

      // Auto-switch to first enabled provider after a short delay
      const firstEnabled = Object.entries(settings.providers)
        .find(([_, config]) => config.enabled)?.[0];
      if (firstEnabled) {
        setTimeout(() => {
          if (activeTab === 'welcome') {
            switchTab(firstEnabled);
          }
        }, 1500);
      } else {
        activeTab = 'settings';
      }

      // Listen for navigation updates
      agentHub.onProviderNavigated((data: any) => {
        console.log('[Proxima UI] Provider navigated:', data);
      });
    } catch (e: any) {
      console.error('[Proxima UI] Init failed:', e);
    }
  });

  // --- Actions ---

  async function switchTab(tabName: string) {
    stopAnalyticsPolling();

    if (tabName === 'settings') {
      activeTab = 'settings';
      await agentHub.hideBrowser();
      return;
    }

    if (tabName === 'analytics') {
      activeTab = 'analytics';
      await agentHub.hideBrowser();
      startAnalyticsPolling();
      return;
    }

    // Check if provider is enabled
    const providerConfig = (settings.providers as any)[tabName];
    if (!providerConfig || !providerConfig.enabled) {
      showToast(`${tabName} is disabled. Enable it in Settings.`);
      return;
    }

    activeTab = tabName;
    await agentHub.showProvider(tabName);
    updateProviderStatus(tabName);
  }

  async function updateProviderStatus(provider: string) {
    try {
      const result = await agentHub.checkLoginStatus(provider);
      (loginStates as any)[provider] = result.loggedIn;
    } catch (e) {
      (loginStates as any)[provider] = false;
    }
  }

  async function checkAllStatus() {
    for (const name of Object.keys(settings.providers)) {
      const config = (settings.providers as any)[name];
      if (config.enabled) {
        await updateProviderStatus(name);
      }
    }
  }

  async function toggleProvider(provider: string) {
    const config = (settings.providers as any)[provider];
    if (!config) return;

    config.enabled = !config.enabled;
    settings = { ...settings };

    await agentHub.saveSettings(settings);
    await agentHub.saveEnabledProviders();

    if (config.enabled) {
      await agentHub.initProvider(provider);
      showToast(`${provider} enabled and initializing...`);
      updateProviderStatus(provider);
    } else {
      showToast(`${provider} disabled`);
      if (activeTab === provider) {
        // Switch to next available or settings
        const next = Object.entries(settings.providers)
          .find(([_, conf]) => conf.enabled)?.[0] || 'settings';
        switchTab(next);
      }
    }
  }

  async function toggleRestApi() {
    settings.restApiEnabled = !settings.restApiEnabled;
    settings = { ...settings };

    await agentHub.saveSettings(settings);
    await agentHub.setRestApiEnabled(settings.restApiEnabled);

    if (settings.restApiEnabled) {
      showToast('REST API server started at http://localhost:3210');
    } else {
      showToast('REST API server stopped');
    }
  }

  async function toggleFileReference() {
    settings.fileReferenceEnabled = !settings.fileReferenceEnabled;
    settings = { ...settings };

    await agentHub.saveSettings(settings);
    await agentHub.setFileReferenceEnabled(settings.fileReferenceEnabled);
    showToast(`File references ${settings.fileReferenceEnabled ? 'enabled' : 'disabled'}`);
  }

  // --- CLI Management ---

  void function updateCliUI() {
    if (cliInstalled) {
      cliButtonText = '✅ Installed';
      cliButtonDisabled = true;
      cliInstallStatus = '✅ CLI is active. Use "proxima" from any terminal.';
    } else {
      cliButtonText = '⚡ Install CLI to PATH';
      cliButtonDisabled = false;
      cliInstallStatus = '';
    }
  }

  async function installCli() {
    cliButtonText = '⏳ Installing...';
    cliButtonDisabled = true;
    try {
      const result = await agentHub.installCli();
      if (result.success) {
        cliInstalled = true;
        updateCliUI();
        showToast('CLI installed — open a new terminal to use it');
      } else {
        cliInstallStatus = '❌ ' + (result.error || 'Install failed');
        cliButtonText = '⚡ Install CLI to PATH';
        cliButtonDisabled = false;
      }
    } catch (err: any) {
      cliInstallStatus = '❌ ' + err.message;
      cliButtonText = '⚡ Install CLI to PATH';
      cliButtonDisabled = false;
    }
  }

  async function uninstallCli() {
    try {
      const result = await agentHub.uninstallCli();
      if (result.success) {
        cliInstalled = false;
        updateCliUI();
        showToast('CLI uninstalled');
      } else {
        cliInstallStatus = '❌ ' + (result.error || 'Uninstall failed');
      }
    } catch (err: any) {
      cliInstallStatus = '❌ ' + err.message;
    }
  }

  // --- Copy Actions ---

  async function copyConfig() {
    const configStr = JSON.stringify(mcpConfig, null, 2);
    await agentHub.copyToClipboard(configStr);
    showToast('Config copied to clipboard!');
  }

  async function copyAIPrompt() {
    const prompt = `Use these MCP tools for local AI queries: \n` + JSON.stringify(mcpConfig, null, 2);
    await agentHub.copyToClipboard(prompt);
    showToast('AI Prompt copied!');
  }

  // --- Page Controls ---

  async function reloadPage() {
    if (activeTab !== 'settings' && activeTab !== 'analytics' && activeTab !== 'welcome') {
      await agentHub.reloadProvider(activeTab);
      showToast(`Reloading ${activeTab}...`);
      setTimeout(() => updateProviderStatus(activeTab), 3000);
    }
  }

  async function openInChrome() {
    if (activeTab !== 'settings' && activeTab !== 'analytics' && activeTab !== 'welcome') {
      await agentHub.openInSystemBrowser(activeTab);
      showToast(`Opening ${activeTab} in system browser for login...`);
    }
  }

  // --- Cookie Modal ---

  function openCookieModal() {
    if (activeTab === 'settings' || activeTab === 'analytics' || activeTab === 'welcome') {
      showToast('Select a provider tab first');
      return;
    }
    cookieModalProvider = activeTab;
    cookieText = '';
    cookieStatus = '';
    cookieStatusClass = '';
    cookieButtonText = '🔓 Login with Cookies';
    cookieButtonDisabled = false;
    showCookieModal = true;
    agentHub.hideBrowser();
  }

  async function closeCookieModal() {
    showCookieModal = false;
    if (activeTab !== 'settings' && activeTab !== 'analytics' && activeTab !== 'welcome') {
      await agentHub.showProvider(activeTab);
    }
  }

  async function applyCookieLogin() {
    const jsonStr = cookieText.trim();
    if (!jsonStr) {
      cookieStatusClass = 'error';
      cookieStatus = '❌ Please paste your cookies JSON first';
      return;
    }

    try {
      const parsed = JSON.parse(jsonStr);
      if (!Array.isArray(parsed) || parsed.length === 0) {
        throw new Error('JSON is not a non-empty array');
      }
    } catch (e) {
      cookieStatusClass = 'error';
      cookieStatus = '❌ Invalid JSON format. Make sure you copy the exported array of cookies.';
      return;
    }

    cookieButtonDisabled = true;
    cookieButtonText = '⏳ Applying cookies...';
    cookieStatus = '';

    try {
      const result = await agentHub.setCookies(cookieModalProvider, jsonStr);
      if (result.success) {
        cookieStatusClass = 'success';
        cookieStatus = `✅ ${result.message}`;
        setTimeout(async () => {
          await closeCookieModal();
          await updateProviderStatus(cookieModalProvider);
          showToast(`${cookieModalProvider} cookies applied!`);
        }, 1500);
      } else {
        cookieStatusClass = 'error';
        cookieStatus = `❌ ${result.error}`;
        cookieButtonDisabled = false;
        cookieButtonText = '🔓 Login with Cookies';
      }
    } catch (e: any) {
      cookieStatusClass = 'error';
      cookieStatus = `❌ Error: ${e.message}`;
      cookieButtonDisabled = false;
      cookieButtonText = '🔓 Login with Cookies';
    }
  }
  const placeholderText = '[{"name":"...", "value":"..."}]';
</script>

{#if isSpotlight}
  <Spotlight />
{:else}
<div class="app-container">
  <!-- Header -->
  <header class="header">
    <div class="header-left">
      <span class="logo">Proxima</span>
      <span class="version">v4.1.0</span>
    </div>
    <div class="header-right">
      {#if activeTab !== 'settings' && activeTab !== 'analytics' && activeTab !== 'welcome'}
        <button class="header-btn" on:click={reloadPage}>🔄 Reload Tab</button>
        <button class="header-btn" on:click={openInChrome}>🌐 Open in Chrome</button>
        <button class="header-btn" on:click={openCookieModal}>🔑 Cookie Login</button>
      {/if}
      <button class="header-btn outline" on:click={() => switchTab('settings')}>⚙️ Settings</button>
    </div>
  </header>

  <!-- Tab Navigation -->
  <nav class="tab-nav">
    {#each Object.keys(settings.providers) as name}
      {@const enabled = (settings.providers as any)[name].enabled}
      <button 
        class="tab {activeTab === name ? 'active' : ''} {!enabled ? 'disabled' : ''}" 
        on:click={() => switchTab(name)}
      >
        <span class="tab-icon {name}">{name.substring(0, 2).toUpperCase()}</span>
        <span class="tab-name">{name.charAt(0).toUpperCase() + name.slice(1)}</span>
        {#if enabled}
          <span class="tab-status {(loginStates as any)[name] ? 'ready' : 'loading'}"></span>
        {/if}
      </button>
    {/each}
    <div class="tab-spacer"></div>
    <button class="tab {activeTab === 'analytics' ? 'active' : ''}" on:click={() => switchTab('analytics')}>
      <span class="tab-icon analytics">📊</span>
      <span class="tab-name">Analytics</span>
    </button>
  </nav>

  <!-- Main View Panel -->
  <main class="main-content">
    
    <!-- Welcome Panel -->
    {#if activeTab === 'welcome'}
      <div class="welcome-panel">
        <h2 class="welcome-title">Welcome to Proxima</h2>
        <p class="welcome-subtitle">Unified Local AI Gateway without API keys. Log in to your provider accounts to get started.</p>
        <div class="welcome-divider"></div>
        <div class="welcome-steps">
          <div class="welcome-step">
            <div class="welcome-step-num">1</div>
            <div class="welcome-step-content">
              <div class="welcome-step-title">Select a Provider</div>
              <div class="welcome-step-desc">Enable your preferred AI model provider in settings.</div>
            </div>
          </div>
          <div class="welcome-step">
            <div class="welcome-step-num">2</div>
            <div class="welcome-step-content">
              <div class="welcome-step-title">Authenticate Account</div>
              <div class="welcome-step-desc">Log into the web interfaces inside the tab window.</div>
            </div>
          </div>
          <div class="welcome-step">
            <div class="welcome-step-num">3</div>
            <div class="welcome-step-content">
              <div class="welcome-step-title">Start Coding</div>
              <div class="welcome-step-desc">Hook Proxima up to Cursor/VS Code and ask queries.</div>
            </div>
          </div>
        </div>
      </div>
    {/if}

    <!-- Webview Container (for active providers) -->
    <div id="browser-container" style="display: { (activeTab !== 'settings' && activeTab !== 'analytics' && activeTab !== 'welcome') ? 'block' : 'none' };">
      <!-- Browser view overlays here -->
      <div class="webview-placeholder">
        <p>Interactive web session tab. If it doesn't render, click reload.</p>
      </div>
    </div>

    <!-- Settings Panel -->
    {#if activeTab === 'settings'}
      <div class="settings-panel">
        
        <!-- Providers Section -->
        <section class="settings-section">
          <h3 class="settings-title">
            <span class="section-icon" style="background: #a855f7;">Providers</span>
            AI Provider Sessions
          </h3>
          <div class="provider-grid">
            {#each Object.keys(settings.providers) as name}
              {@const enabled = (settings.providers as any)[name].enabled}
              <div class="provider-card {enabled ? 'enabled' : 'disabled'}">
                <div class="provider-card-icon {name}">{name.substring(0,2).toUpperCase()}</div>
                <div class="provider-card-info">
                  <div class="provider-card-name">{name.charAt(0).toUpperCase() + name.slice(1)}</div>
                  <div class="provider-card-status {enabled && (loginStates as any)[name] ? 'ready' : 'not-ready'}">
                    {#if enabled}
                      {(loginStates as any)[name] ? '✅ Active Session' : '⏳ Checking authentication...'}
                    {:else}
                      ❌ Disabled
                    {/if}
                  </div>
                </div>
                <div class="toggle-switch {enabled ? 'active' : ''}" on:click={() => toggleProvider(name)}></div>
              </div>
            {/each}
          </div>
        </section>

        <!-- Gateway Config Section -->
        <section class="settings-section">
          <h3 class="settings-title">
            <span class="section-icon" style="background: #22c55e;">Gateway</span>
            Local API & CLI Server
          </h3>
          
          <div class="settings-list">
            <div class="feature-toggle-row">
              <div class="feature-info">
                <div class="feature-name">Enable REST API & WebSocket Server</div>
                <div class="feature-desc">Hosts an OpenAI-compatible completion endpoint on port 3210.</div>
              </div>
              <div class="toggle-switch {settings.restApiEnabled ? 'active' : ''}" on:click={toggleRestApi}></div>
            </div>

            <div class="feature-toggle-row" style="margin-top: 14px;">
              <div class="feature-info">
                <div class="feature-name">Code File References</div>
                <div class="feature-desc">Allows coding assistants to load local project files directly.</div>
              </div>
              <div class="toggle-switch {settings.fileReferenceEnabled ? 'active' : ''}" on:click={toggleFileReference}></div>
            </div>
          </div>
        </section>

        <!-- CLI Installation Section -->
        <section class="settings-section">
          <h3 class="settings-title">
            <span class="section-icon" style="background: #06b6d4;">CLI</span>
            Terminal CLI Tool
          </h3>
          <div class="cli-actions">
            <button class="action-btn" on:click={installCli} disabled={cliButtonDisabled}>{cliButtonText}</button>
            {#if cliInstalled}
              <button class="action-btn danger" on:click={uninstallCli}>Uninstall CLI</button>
            {/if}
            {#if cliInstallStatus}
              <p class="cli-status-msg">{cliInstallStatus}</p>
            {/if}
          </div>
        </section>

        <!-- MCP config section -->
        <section class="settings-section">
          <h3 class="settings-title">
            <span class="section-icon" style="background: #eab308;">MCP</span>
            Model Context Protocol Configuration
          </h3>
          <div class="mcp-tabs">
            <button class="method-btn {configMethod === 'manual' ? 'active' : ''}" on:click={() => configMethod = 'manual'}>Manual Setup</button>
            <button class="method-btn {configMethod === 'ai' ? 'active' : ''}" on:click={() => configMethod = 'ai'}>Copy Prompt for AI</button>
          </div>
          
          <div class="mcp-content" style="margin-top: 14px;">
            {#if configMethod === 'manual'}
              <div class="config-box">
                <pre>{JSON.stringify(mcpConfig, null, 2)}</pre>
                <button class="copy-btn" on:click={copyConfig}>📋 Copy Configuration</button>
              </div>
            {:else}
              <div class="config-box">
                <pre>Configure these local tools in your assistant: {JSON.stringify(mcpConfig, null, 2)}</pre>
                <button class="copy-btn" on:click={copyAIPrompt}>📋 Copy Prompt</button>
              </div>
            {/if}
          </div>
        </section>

      </div>
    {/if}

    <!-- Analytics Dashboard -->
    {#if activeTab === 'analytics'}
      <div class="analytics-panel">
        
        <!-- Header & Quick Stats Cards -->
        <section class="analytics-section">
          <div class="analytics-header">
            <h3 class="analytics-title">
              <span class="section-icon" style="background: #ec4899;">📊</span>
              Performance & Session Analytics
            </h3>
            <div class="export-buttons">
              <button class="action-btn outline" on:click={exportAsJSON}>📥 Export JSON</button>
              <button class="action-btn outline" on:click={exportAsMarkdown}>📝 Export Markdown</button>
              <button class="action-btn outline" on:click={loadAnalytics}>🔄 Refresh</button>
            </div>
          </div>
          
          <div class="stats-grid">
            <div class="stats-card">
              <div class="stats-card-label">Uptime</div>
              <div class="stats-card-val">{statsData.uptime}</div>
            </div>
            <div class="stats-card">
              <div class="stats-card-label">Total Requests</div>
              <div class="stats-card-val">{statsData.totalRequests}</div>
            </div>
            <div class="stats-card">
              <div class="stats-card-label">Total Errors</div>
              <div class="stats-card-val error-text">{statsData.totalErrors}</div>
            </div>
            <div class="stats-card">
              <div class="stats-card-label">Success Rate</div>
              <div class="stats-card-val success-text">
                {statsData.totalRequests > 0 ? ((1 - (statsData.totalErrors / statsData.totalRequests)) * 100).toFixed(1) + '%' : '100%'}
              </div>
            </div>
          </div>
        </section>

        <!-- Provider Metrics Table -->
        <section class="analytics-section">
          <h4 class="section-subtitle">Provider Performance Breakdowns</h4>
          <div class="table-container">
            <table class="analytics-table">
              <thead>
                <tr>
                  <th>Provider</th>
                  <th>Calls</th>
                  <th>Errors</th>
                  <th>Avg Response Time</th>
                  <th>Min/Max Time</th>
                  <th>Last Call</th>
                </tr>
              </thead>
              <tbody>
                {#if Object.keys(statsData.providers).length === 0}
                  <tr>
                    <td colspan="6" class="table-empty">No calls recorded yet. Send queries using Proxima or CLI to see metrics.</td>
                  </tr>
                {:else}
                  {#each Object.entries(statsData.providers) as [providerName, pStats]}
                    <tr>
                      <td>
                        <span class="provider-badge {providerName.split('/')[0]}">
                          {providerName}
                        </span>
                      </td>
                      <td>{pStats.calls}</td>
                      <td class={pStats.errors > 0 ? 'error-text' : ''}>{pStats.errors}</td>
                      <td>{(pStats.avg_time_ms / 1000).toFixed(2)}s</td>
                      <td>{(pStats.min_time_ms / 1000).toFixed(2)}s / {(pStats.max_time_ms / 1000).toFixed(2)}s</td>
                      <td>{pStats.last_call ? new Date(pStats.last_call).toLocaleTimeString() : 'N/A'}</td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </section>

        <!-- Query History Log -->
        <section class="analytics-section">
          <h4 class="section-subtitle">Request History Logs</h4>
          <div class="history-container">
            {#if historyData.length === 0}
              <div class="history-empty">No query history found.</div>
            {:else}
              <div class="history-list">
                {#each historyData.slice().reverse() as item}
                  <div class="history-row">
                    <div class="history-meta">
                      <span class="history-time">{new Date(item.timestamp).toLocaleString()}</span>
                      <span class="history-badge {item.model.split('/')[0]}">{item.model}</span>
                    </div>
                    <div class="history-content">
                      <div class="history-query"><strong>Q:</strong> {item.query}</div>
                      {#if item.response}
                        <div class="history-response"><strong>A:</strong> {item.response}</div>
                      {/if}
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        </section>

      </div>
    {/if}

  </main>
</div>

<!-- Cookie Modal -->
{#if showCookieModal}
  <div class="cookie-modal-overlay active" on:click={closeCookieModal}>
    <div class="cookie-modal" on:click|stopPropagation>
      <div class="cookie-modal-header">
        <h4 class="cookie-modal-title">🔑 Cookie Login: <span id="cookie-modal-provider">{cookieModalProvider}</span></h4>
        <button class="cookie-modal-close" on:click={closeCookieModal}>&times;</button>
      </div>
      <div class="cookie-instructions">
        <strong>How to authenticate using cookies:</strong>
        <ol>
          <li>Install a cookie exporter browser extension (e.g. <code>EditThisCookie</code>).</li>
          <li>Log in to the provider website in your system browser (Chrome/Firefox).</li>
          <li>Export cookies as a JSON array.</li>
          <li>Paste the JSON array below and click apply.</li>
        </ol>
      </div>
      <textarea 
        class="cookie-textarea" 
        placeholder={placeholderText}
        bind:value={cookieText}
      ></textarea>
      {#if cookieStatus}
        <div class="cookie-status {cookieStatusClass}">{cookieStatus}</div>
      {/if}
      <div class="cookie-modal-actions" style="margin-top: 14px; display: flex; justify-content: flex-end; gap: 10px;">
        <button class="action-btn cancel" on:click={closeCookieModal}>Cancel</button>
        <button class="action-btn" on:click={applyCookieLogin} disabled={cookieButtonDisabled}>{cookieButtonText}</button>
      </div>
    </div>
  </div>
{/if}

<!-- Toast -->
{#if toast}
  <div class="toast" style="display: block;">{toast}</div>
{/if}
{/if}

<style>
  .app-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #0f0f23;
    color: #fff;
    font-family: 'Inter', sans-serif;
  }

  /* Header */
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    background: rgba(0, 0, 0, 0.4);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .logo {
    font-size: 1.4rem;
    font-weight: 800;
    background: linear-gradient(135deg, #00d4ff, #7c3aed, #f472b6);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }

  .version {
    background: rgba(255, 255, 255, 0.08);
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 0.7rem;
    font-weight: 600;
    color: #a78bfa;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .header-btn {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    padding: 6px 14px;
    border-radius: 8px;
    color: white;
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 500;
    transition: all 0.2s;
  }

  .header-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .header-btn.outline {
    background: linear-gradient(135deg, rgba(168, 85, 247, 0.15), rgba(244, 114, 182, 0.15));
    border-color: rgba(168, 85, 247, 0.3);
  }

  .header-btn.outline:hover {
    border-color: rgba(168, 85, 247, 0.6);
    background: linear-gradient(135deg, rgba(168, 85, 247, 0.25), rgba(244, 114, 182, 0.25));
  }

  /* Tab Navigation */
  .tab-nav {
    display: flex;
    align-items: center;
    background: rgba(0, 0, 0, 0.25);
    padding: 8px 12px;
    gap: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.05);
    color: #9ca3af;
    cursor: pointer;
    transition: all 0.2s;
    position: relative;
  }

  .tab:hover:not(.disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
  }

  .tab.active {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.15);
    color: #fff;
  }

  .tab.disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .tab-icon {
    font-size: 0.75rem;
    font-weight: 700;
    padding: 3px 6px;
    border-radius: 4px;
    color: #fff;
  }

  .tab-icon.perplexity { background: linear-gradient(135deg, #20b2aa, #00ced1); }
  .tab-icon.chatgpt { background: linear-gradient(135deg, #10a37f, #1a7f64); }
  .tab-icon.claude { background: linear-gradient(135deg, #cc785c, #d4956c); }
  .tab-icon.gemini { background: linear-gradient(135deg, #4285f4, #8ab4f8); }
  .tab-icon.deepseek { background: linear-gradient(135deg, #0d53c0, #0a84ff); }
  .tab-icon.kimi { background: linear-gradient(135deg, #2ea97d, #00c9a7); }
  .tab-icon.analytics { background: transparent; font-size: 1.1rem; padding: 0; }

  .tab-name {
    font-weight: 500;
    font-size: 0.85rem;
  }

  .tab-status {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    position: absolute;
    top: 5px;
    right: 5px;
  }

  .tab-status.ready {
    background: #22c55e;
    box-shadow: 0 0 6px #22c55e;
  }

  .tab-status.loading {
    background: #f59e0b;
    animation: pulse 1.2s infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .tab-spacer {
    flex: 1;
  }

  /* Main Content */
  .main-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
  }

  #browser-container {
    flex: 1;
    background: #111124;
    position: relative;
  }

  .webview-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #4b5563;
    font-size: 0.9rem;
  }

  /* Settings Panel */
  .settings-panel {
    flex: 1;
    padding: 24px 30px;
    overflow-y: auto;
    background: #0d0d1f;
  }

  .settings-section {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 14px;
    padding: 20px;
    margin-bottom: 20px;
  }

  .settings-title {
    font-size: 1.15rem;
    font-weight: 700;
    margin-bottom: 18px;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .section-icon {
    font-size: 0.65rem;
    font-weight: 700;
    padding: 3px 8px;
    border-radius: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  /* Provider Card Grid */
  .provider-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 14px;
  }

  .provider-card {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    align-items: center;
    gap: 14px;
    transition: all 0.2s ease;
  }

  .provider-card.enabled {
    border-color: rgba(34, 197, 94, 0.25);
    background: rgba(34, 197, 94, 0.02);
  }

  .provider-card.disabled {
    opacity: 0.5;
  }

  .provider-card-icon {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.9rem;
    font-weight: 700;
    color: #fff;
  }

  .provider-card-icon.perplexity { background: linear-gradient(135deg, #20b2aa, #00ced1); }
  .provider-card-icon.chatgpt { background: linear-gradient(135deg, #10a37f, #1a7f64); }
  .provider-card-icon.claude { background: linear-gradient(135deg, #cc785c, #d4956c); }
  .provider-card-icon.gemini { background: linear-gradient(135deg, #4285f4, #8ab4f8); }
  .provider-card-icon.deepseek { background: linear-gradient(135deg, #0d53c0, #0a84ff); }
  .provider-card-icon.kimi { background: linear-gradient(135deg, #2ea97d, #00c9a7); }

  .provider-card-info {
    flex: 1;
  }

  .provider-card-name {
    font-weight: 600;
    font-size: 0.95rem;
  }

  .provider-card-status {
    font-size: 0.75rem;
    color: #6b7280;
    margin-top: 2px;
  }

  .provider-card-status.ready {
    color: #22c55e;
  }

  /* Toggles */
  .toggle-switch {
    width: 42px;
    height: 22px;
    background: #2e303d;
    border-radius: 11px;
    position: relative;
    cursor: pointer;
    transition: background 0.2s;
  }

  .toggle-switch.active {
    background: #22c55e;
  }

  .toggle-switch::after {
    content: '';
    position: absolute;
    width: 18px;
    height: 18px;
    background: #fff;
    border-radius: 50%;
    top: 2px;
    left: 2px;
    transition: transform 0.2s;
  }

  .toggle-switch.active::after {
    transform: translateX(20px);
  }

  /* Feature Toggles */
  .feature-toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.04);
    padding: 14px 16px;
    border-radius: 10px;
  }

  .feature-name {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .feature-desc {
    font-size: 0.8rem;
    color: #6b7280;
    margin-top: 3px;
  }

  /* CLI Controls */
  .cli-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .action-btn {
    background: linear-gradient(135deg, #06b6d4, #0891b2);
    color: #fff;
    border: none;
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }

  .action-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .action-btn:disabled {
    cursor: not-allowed;
  }

  .action-btn.danger {
    background: rgba(239, 68, 68, 0.1);
    color: #ef4444;
    border: 1px solid rgba(239, 68, 68, 0.25);
  }

  .action-btn.danger:hover {
    background: rgba(239, 68, 68, 0.2);
  }

  .action-btn.cancel {
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.1);
  }

  .action-btn.cancel:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .cli-status-msg {
    font-size: 0.8rem;
    color: #22c55e;
  }

  /* MCP Configuration tab */
  .mcp-tabs {
    display: flex;
    gap: 8px;
  }

  .method-btn {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 6px 14px;
    border-radius: 6px;
    color: #6b7280;
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 500;
    transition: all 0.2s;
  }

  .method-btn.active {
    background: rgba(167, 139, 250, 0.15);
    border-color: rgba(167, 139, 250, 0.3);
    color: #c084fc;
  }

  .config-box {
    background: #090912;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 8px;
    padding: 16px;
    font-family: monospace;
    font-size: 0.8rem;
    position: relative;
    color: #a5f3fc;
  }

  .config-box pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .copy-btn {
    position: absolute;
    top: 10px;
    right: 10px;
    background: rgba(167, 139, 250, 0.2);
    border: 1px solid rgba(167, 139, 250, 0.4);
    color: #c084fc;
    padding: 5px 10px;
    border-radius: 4px;
    font-size: 0.75rem;
    font-weight: 600;
    cursor: pointer;
  }

  .copy-btn:hover {
    background: rgba(167, 139, 250, 0.35);
  }

  /* Welcome view */
  .welcome-panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 40px;
    text-align: center;
    background: radial-gradient(ellipse at top, rgba(168, 85, 247, 0.05) 0%, transparent 60%);
  }

  .welcome-title {
    font-size: 2rem;
    font-weight: 700;
    background: linear-gradient(135deg, #3b82f6, #a855f7);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    margin-bottom: 8px;
  }

  .welcome-subtitle {
    color: #6b7280;
    font-size: 0.95rem;
    max-width: 480px;
    line-height: 1.5;
  }

  .welcome-divider {
    width: 60px;
    height: 1px;
    background: rgba(168, 85, 247, 0.25);
    margin: 30px 0;
  }

  .welcome-steps {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    max-width: 360px;
  }

  .welcome-step {
    display: flex;
    align-items: center;
    gap: 14px;
    background: rgba(255, 255, 255, 0.01);
    border: 1px solid rgba(255, 255, 255, 0.03);
    padding: 12px 18px;
    border-radius: 10px;
    text-align: left;
  }

  .welcome-step-num {
    min-width: 28px;
    height: 28px;
    background: rgba(168, 85, 247, 0.15);
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 0.85rem;
    color: #c084fc;
  }

  .welcome-step-content {
    flex: 1;
  }

  .welcome-step-title {
    font-weight: 600;
    font-size: 0.85rem;
    color: #e5e7eb;
  }

  .welcome-step-desc {
    font-size: 0.75rem;
    color: #6b7280;
    margin-top: 2px;
  }

  /* Cookie Modal Overlay */
  .cookie-modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.75);
    backdrop-filter: blur(4px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .cookie-modal {
    background: #111126;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
    padding: 24px;
    width: 500px;
    max-width: 90%;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  }

  .cookie-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
  }

  .cookie-modal-title {
    font-size: 1.15rem;
    font-weight: 700;
    color: #a78bfa;
  }

  .cookie-modal-close {
    background: transparent;
    border: none;
    color: #6b7280;
    font-size: 1.5rem;
    cursor: pointer;
    line-height: 1;
  }

  .cookie-modal-close:hover {
    color: #fff;
  }

  .cookie-instructions {
    background: rgba(167, 139, 250, 0.05);
    border: 1px solid rgba(167, 139, 250, 0.15);
    border-radius: 8px;
    padding: 12px;
    font-size: 0.8rem;
    line-height: 1.4;
    margin-bottom: 14px;
  }

  .cookie-instructions ol {
    margin: 8px 0 0 16px;
  }

  .cookie-textarea {
    width: 100%;
    height: 120px;
    background: #090912;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    color: #fff;
    padding: 10px;
    font-family: monospace;
    font-size: 0.75rem;
    box-sizing: border-box;
    resize: none;
  }

  .cookie-status {
    font-size: 0.8rem;
    margin-top: 10px;
  }

  .cookie-status.error {
    color: #ef4444;
  }

  .cookie-status.success {
    color: #22c55e;
  }

  /* Toast notification */
  .toast {
    position: fixed;
    bottom: 20px;
    right: 20px;
    background: linear-gradient(135deg, #10b981, #059669);
    color: #fff;
    padding: 12px 20px;
    border-radius: 8px;
    font-size: 0.85rem;
    font-weight: 600;
    box-shadow: 0 4px 15px rgba(16, 185, 129, 0.3);
    z-index: 9999;
  }

  .analytics-coming-soon {
    color: #6b7280;
    font-size: 0.9rem;
    text-align: center;
    padding: 40px 0;
  }

  /* Analytics Dashboard Styles */
  .analytics-panel {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 24px;
    background: #090912;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.05);
    max-height: calc(100vh - 120px);
    overflow-y: auto;
  }

  .analytics-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .analytics-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    padding-bottom: 14px;
  }

  .analytics-title {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: 1.2rem;
    font-weight: 600;
  }

  .export-buttons {
    display: flex;
    gap: 10px;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .stats-card {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
  }

  .stats-card-label {
    font-size: 0.8rem;
    color: #9ca3af;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .stats-card-val {
    font-size: 1.6rem;
    font-weight: 700;
    color: #f3f4f6;
  }

  .error-text {
    color: #ef4444 !important;
  }

  .success-text {
    color: #10b981 !important;
  }

  .section-subtitle {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: #a78bfa;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    padding-bottom: 8px;
  }

  .table-container {
    background: rgba(255, 255, 255, 0.01);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 10px;
    overflow-x: auto;
  }

  .analytics-table {
    width: 100%;
    border-collapse: collapse;
    text-align: left;
    font-size: 0.9rem;
  }

  .analytics-table th, .analytics-table td {
    padding: 12px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .analytics-table th {
    background: rgba(255, 255, 255, 0.03);
    color: #9ca3af;
    font-weight: 600;
    text-transform: uppercase;
    font-size: 0.75rem;
    letter-spacing: 0.05em;
  }

  .analytics-table tr:hover {
    background: rgba(255, 255, 255, 0.015);
  }

  .table-empty {
    color: #6b7280;
    text-align: center;
    padding: 24px;
  }

  .provider-badge {
    display: inline-block;
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 0.8rem;
    font-weight: 600;
    color: #fff;
    text-transform: capitalize;
  }

  /* Badges match active provider styling */
  .provider-badge.perplexity, .history-badge.perplexity { background: linear-gradient(135deg, #20b2aa, #00ced1); }
  .provider-badge.chatgpt, .history-badge.chatgpt { background: linear-gradient(135deg, #10a37f, #1a7f64); }
  .provider-badge.claude, .history-badge.claude { background: linear-gradient(135deg, #cc785c, #d4956c); }
  .provider-badge.gemini, .history-badge.gemini { background: linear-gradient(135deg, #4285f4, #8ab4f8); }
  .provider-badge.deepseek, .history-badge.deepseek { background: linear-gradient(135deg, #0d53c0, #0a84ff); }
  .provider-badge.kimi, .history-badge.kimi { background: linear-gradient(135deg, #2ea97d, #00c9a7); }
  .provider-badge.ollama, .history-badge.ollama { background: linear-gradient(135deg, #ec4899, #a855f7); }

  .history-container {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: 400px;
    overflow-y: auto;
    background: rgba(255, 255, 255, 0.01);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 10px;
    padding: 16px;
  }

  .history-empty {
    color: #6b7280;
    text-align: center;
    padding: 24px;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .history-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.04);
  }

  .history-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .history-time {
    font-size: 0.75rem;
    color: #6b7280;
  }

  .history-badge {
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 0.7rem;
    font-weight: 600;
    color: #fff;
  }

  .history-content {
    font-size: 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .history-query {
    color: #e5e7eb;
    white-space: pre-wrap;
  }

  .history-response {
    color: #9ca3af;
    background: rgba(0, 0, 0, 0.2);
    padding: 10px;
    border-radius: 6px;
    border-left: 3px solid #a855f7;
    white-space: pre-wrap;
  }
</style>
