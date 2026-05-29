<script lang="ts">
  import { onMount, tick } from 'svelte';

  let query = '';
  let model = 'auto';
  let responseText = '';
  let isQuerying = false;
  let statusText = '';
  let responseContainer: HTMLDivElement;

  const agentHub = (window as any).agentHub;

  const models = [
    { id: 'auto', name: 'Auto (Best)' },
    { id: 'chatgpt', name: 'ChatGPT' },
    { id: 'claude', name: 'Claude' },
    { id: 'gemini', name: 'Gemini' },
    { id: 'perplexity', name: 'Perplexity' },
    { id: 'deepseek', name: 'DeepSeek' },
    { id: 'kimi', name: 'Kimi' }
  ];

  async function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      if (agentHub) {
        await agentHub.hideSpotlight();
      }
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      await submitQuery();
    }
  }

  async function submitQuery() {
    const trimmed = query.trim();
    if (!trimmed || isQuerying) return;

    responseText = '';
    isQuerying = true;
    statusText = `Querying ${model}...`;

    try {
      // Use local REST server port 3210
      const res = await fetch('http://127.0.0.1:3210/v1/chat/completions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model,
          message: trimmed
        })
      });

      if (!res.ok) {
        const errJson = await res.json().catch(() => ({}));
        throw new Error(errJson.error || `HTTP error ${res.status}`);
      }

      const data = await res.json();
      if (data.choices && data.choices[0] && data.choices[0].message) {
        responseText = data.choices[0].message.content;
        statusText = 'Done';
      } else {
        responseText = JSON.stringify(data, null, 2);
        statusText = 'Done';
      }
    } catch (e: any) {
      responseText = `Error: ${e.message}`;
      statusText = 'Failed';
    } finally {
      isQuerying = false;
      await tick();
      if (responseContainer) {
        responseContainer.scrollTop = responseContainer.scrollHeight;
      }
    }
  }

  onMount(() => {
    // Focus the input automatically
    const input = document.getElementById('spotlight-input');
    if (input) input.focus();

    // Listen for Escape globally in window
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  });
</script>

<div class="spotlight-wrapper">
  <!-- Input Bar -->
  <div class="spotlight-input-container">
    <span class="spotlight-icon">⚡</span>
    <input
      id="spotlight-input"
      type="text"
      placeholder="Ask Synapse anything... (Esc to close)"
      bind:value={query}
      on:keydown={handleKeyDown}
      disabled={isQuerying}
      autocomplete="off"
    />
    {#if isQuerying}
      <div class="spotlight-spinner"></div>
    {/if}
  </div>

  <!-- Settings / Options Row -->
  <div class="spotlight-options-row">
    <div class="spotlight-model-selector">
      <span class="selector-label">Model:</span>
      <select bind:value={model} disabled={isQuerying}>
        {#each models as m}
          <option value={m.id}>{m.name}</option>
        {/each}
      </select>
    </div>
    {#if statusText}
      <div class="spotlight-status">{statusText}</div>
    {/if}
  </div>

  <!-- Results Output Area -->
  {#if responseText || isQuerying}
    <div class="spotlight-results" bind:this={responseContainer}>
      {#if isQuerying && !responseText}
        <div class="spotlight-placeholder-text">Waiting for response...</div>
      {/if}
      {#if responseText}
        <div class="spotlight-markdown-content">
          {responseText}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent !important;
    overflow: hidden;
  }

  .spotlight-wrapper {
    background: rgba(15, 15, 35, 0.75);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5), 0 0 20px rgba(168, 85, 247, 0.1);
    color: #fff;
    font-family: 'Inter', sans-serif;
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    overflow: hidden;
  }

  .spotlight-input-container {
    display: flex;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .spotlight-icon {
    font-size: 1.25rem;
    margin-right: 12px;
  }

  input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: #fff;
    font-size: 1.1rem;
    font-weight: 500;
  }

  input::placeholder {
    color: rgba(255, 255, 255, 0.35);
  }

  .spotlight-spinner {
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.1);
    border-top: 2px solid #a855f7;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }

  .spotlight-options-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 18px;
    background: rgba(0, 0, 0, 0.2);
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    font-size: 0.8rem;
  }

  .spotlight-model-selector {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .selector-label {
    color: rgba(255, 255, 255, 0.4);
    font-weight: 500;
  }

  select {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    color: #fff;
    outline: none;
    padding: 2px 6px;
    font-size: 0.8rem;
    cursor: pointer;
  }

  select option {
    background: #0f0f23;
    color: #fff;
  }

  .spotlight-status {
    color: #a855f7;
    font-weight: 500;
  }

  .spotlight-results {
    flex: 1;
    overflow-y: auto;
    padding: 16px 18px;
    background: rgba(0, 0, 0, 0.15);
    font-size: 0.95rem;
    line-height: 1.5;
  }

  .spotlight-placeholder-text {
    color: rgba(255, 255, 255, 0.4);
    font-style: italic;
  }

  .spotlight-markdown-content {
    white-space: pre-wrap;
    color: rgba(255, 255, 255, 0.85);
  }
</style>
