/**
 * Synapse — DeepSeek Engine v4.1.0
 * Runs inside chat.deepseek.com BrowserView context. Uses session cookies for auth,
 * creates persistent conversations, and streams responses via SSE.
 */
(function() {
    if (window.__synapseDeepSeek) return;

    const DEEPSEEK_BASE = 'https://chat.deepseek.com';
    var TIMEOUT = 360000;
    let _chatSessionId = null;

    // ─── Organization / Session ───────────────────────
    async function _checkSession() {
        const res = await fetch('/api/v0/user/profile', { credentials: 'include' });
        if (res.status === 401 || res.status === 403) {
            throw new Error('Not logged in to DeepSeek');
        }
        if (!res.ok) throw new Error('DeepSeek session check failed');
        return true;
    }

    // ─── Conversation ────────────────────────────────
    async function _createConversation(prompt) {
        const res = await fetch('/api/v0/chat_session/create', {
            method: 'POST',
            credentials: 'include',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                character_id: null
            })
        });
        if (!res.ok) {
            throw new Error('DeepSeek conversation creation failed');
        }
        const data = await res.json();
        return data.data.biz_data.chat_session.chat_session_id;
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
            _chatSessionId = await _createConversation(message);
            console.log('[Synapse DeepSeek] Created new session:', _chatSessionId);
        }

        try {
            var controller = new AbortController();
            var timeoutId = setTimeout(function() { controller.abort(); }, TIMEOUT);

            var res = await fetch('/api/v0/chat/completion', {
                method: 'POST',
                credentials: 'include',
                headers: {
                    'Content-Type': 'application/json',
                    'Accept': 'text/event-stream'
                },
                body: JSON.stringify({
                    chat_session_id: _chatSessionId,
                    prompt: message,
                    model: "deepseek-chat"
                }),
                signal: controller.signal
            });

            if (!res.ok) {
                clearTimeout(timeoutId);
                var errBody = await res.text().catch(function() { return ''; });
                if (res.status === 404) {
                    // Retry once with a new conversation
                    _chatSessionId = await _createConversation(message);
                    return await fn_send(message);
                }
                throw new Error('DeepSeek API failed (' + res.status + '): ' + errBody.substring(0, 200));
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
        console.log('[Synapse DeepSeek] Conversation reset');
    }

    window.__synapseDeepSeek = { send: fn_send, newConversation: newConversation };
    console.log('[Synapse] DeepSeek engine loaded');
})();
