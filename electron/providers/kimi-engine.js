/**
 * Synapse — Kimi Engine v4.1.0
 * Runs inside kimi.moonshot.cn BrowserView context. Uses session cookies for auth,
 * creates persistent conversations, and streams responses via SSE.
 */
(function() {
    if (window.__synapseKimi) return;

    const KIMI_BASE = 'https://kimi.moonshot.cn';
    var TIMEOUT = 360000;
    let _chatSessionId = null;

    // ─── Organization / Session ───────────────────────
    async function _checkSession() {
        const res = await fetch('/api/user/profile', { credentials: 'include' });
        if (res.status === 401 || res.status === 403) {
            throw new Error('Not logged in to Kimi');
        }
        if (!res.ok) throw new Error('Kimi session check failed');
        return true;
    }

    // ─── Conversation ────────────────────────────────
    async function _createConversation() {
        const res = await fetch('/api/chat', {
            method: 'POST',
            credentials: 'include',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                name: "synapse-chat",
                is_starred: false
            })
        });
        if (!res.ok) {
            throw new Error('Kimi conversation creation failed');
        }
        const data = await res.json();
        return data.id;
    }

    // ─── SSE Stream Parser ──────────────────────────
    async function _parseStream(response) {
        var reader = response.body.getReader();
        var decoder = new TextDecoder();
        var fullText = '';
        var buffer = '';

        while (true) {
            var chunk = await reader.read();
            if (chunk.done) break;

            buffer += decoder.decode(chunk.value, { stream: true });
            var lines = buffer.split('\n');
            buffer = lines.pop() || '';

            for (var i = 0; i < lines.length; i++) {
                var line = lines[i];
                if (!line.startsWith('data: ')) continue;
                var data = line.slice(6).trim();
                if (!data || data === '[DONE]') continue;

                try {
                    var parsed = JSON.parse(data);
                    if (parsed.choices && parsed.choices[0].delta && parsed.choices[0].delta.content) {
                        fullText += parsed.choices[0].delta.content;
                    }
                } catch(e) {}
            }
        }

        reader.releaseLock();
        return fullText;
    }

    // ─── Send Message ───────────────────────────────
    async function fn_send(message) {
        await _checkSession();

        if (!_chatSessionId) {
            _chatSessionId = await _createConversation();
            console.log('[Synapse Kimi] Created new session:', _chatSessionId);
        }

        try {
            var controller = new AbortController();
            var timeoutId = setTimeout(function() { controller.abort(); }, TIMEOUT);

            var res = await fetch('/api/chat/' + _chatSessionId + '/completion', {
                method: 'POST',
                credentials: 'include',
                headers: {
                    'Content-Type': 'application/json',
                    'Accept': 'text/event-stream'
                },
                body: JSON.stringify({
                    messages: [{
                        role: "user",
                        content: message
                    }],
                    use_search: true
                }),
                signal: controller.signal
            });

            if (!res.ok) {
                clearTimeout(timeoutId);
                var errBody = await res.text().catch(function() { return ''; });
                if (res.status === 404) {
                    // Retry once with a new conversation
                    _chatSessionId = await _createConversation();
                    return await fn_send(message);
                }
                throw new Error('Kimi API failed (' + res.status + '): ' + errBody.substring(0, 200));
            }

            var result = await _parseStream(res);
            clearTimeout(timeoutId);
            return result;
        } catch(e) {
            throw e;
        }
    }

    function newConversation() {
        _chatSessionId = null;
        console.log('[Synapse Kimi] Conversation reset');
    }

    window.__synapseKimi = { send: fn_send, newConversation: newConversation };
    console.log('[Synapse] Kimi engine loaded');
})();
