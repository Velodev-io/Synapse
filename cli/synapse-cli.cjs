#!/usr/bin/env node

// ─────────────────────────────────────────────────────
// Synapse CLI — Talk to AI from your terminal
// Usage: synapse ask claude "What is AI?"  
//        synapse search "latest AI news"
//        synapse debate "Is AI dangerous?"
// ─────────────────────────────────────────────────────

const http = require('http');
const fs = require('fs');
const path = require('path');
let version = '4.1.0';
try { version = require('../package.json').version; } catch (e) {}

// ─── Config ──────────────────────────────────────────
const API_HOST = process.env.SYNAPSE_HOST || '127.0.0.1';
const API_PORT = parseInt(process.env.SYNAPSE_PORT) || 3210;
const API_BASE = `http://${API_HOST}:${API_PORT}`;

// ─── Colors (ANSI) ──────────────────────────────────
const c = {
    reset: '\x1b[0m',
    bold: '\x1b[1m',
    dim: '\x1b[2m',
    purple: '\x1b[35m',
    cyan: '\x1b[36m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    red: '\x1b[31m',
    blue: '\x1b[34m',
    gray: '\x1b[90m',
    white: '\x1b[37m',
    bgPurple: '\x1b[45m',
};

function colorize(color, text) { return `${color}${text}${c.reset}`; }
function bold(text) { return colorize(c.bold, text); }
function dim(text) { return colorize(c.dim, text); }
function purple(text) { return colorize(c.purple, text); }
function cyan(text) { return colorize(c.cyan, text); }
function green(text) { return colorize(c.green, text); }
function yellow(text) { return colorize(c.yellow, text); }
function red(text) { return colorize(c.red, text); }

// ─── HTTP Client ────────────────────────────────────
function apiRequest(method, path, body = null) {
    return new Promise((resolve, reject) => {
        const url = new URL(path, API_BASE);
        const options = {
            hostname: url.hostname,
            port: url.port,
            path: url.pathname,
            method,
            headers: { 'Content-Type': 'application/json' },
            timeout: 120000 // 2 min timeout for slow providers
        };

        const req = http.request(options, (res) => {
            let data = '';
            res.on('data', chunk => { data += chunk; });
            res.on('end', () => {
                try {
                    resolve({ status: res.statusCode, data: JSON.parse(data) });
                } catch {
                    resolve({ status: res.statusCode, data: { raw: data } });
                }
            });
        });

        req.on('error', (e) => {
            if (e.code === 'ECONNREFUSED') {
                reject(new Error('Cannot connect to Synapse. Is it running? (npm start)'));
            } else {
                reject(e);
            }
        });

        req.on('timeout', () => {
            req.destroy();
            reject(new Error('Request timed out (120s). Provider may be slow.'));
        });

        if (body) req.write(JSON.stringify(body));
        req.end();
    });
}

// ─── Spinner ────────────────────────────────────────
function startSpinner(text) {
    const frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let i = 0;
    const interval = setInterval(() => {
        process.stderr.write(`\r${purple(frames[i++ % frames.length])} ${dim(text)}`);
    }, 80);
    return {
        stop: (final) => {
            clearInterval(interval);
            process.stderr.write(`\r${' '.repeat(text.length + 4)}\r`);
            if (final) process.stderr.write(`${final}\n`);
        }
    };
}

// ─── Format Response ────────────────────────────────
function formatResponse(data) {
    // OpenAI format
    if (data.choices && data.choices.length > 0) {
        const choice = data.choices[0];
        const content = choice.message?.content || choice.text || '';
        const model = data.synapse?.provider || data.model || 'unknown';
        const time = data.synapse?.responseTimeMs;
        
        console.log();
        console.log(`${purple('┌')} ${bold(model.toUpperCase())}${time ? dim(` (${(time/1000).toFixed(1)}s)`) : ''}`);
        console.log(`${purple('│')}`);
        content.split('\n').forEach(line => {
            console.log(`${purple('│')} ${line}`);
        });
        console.log(`${purple('└──────────────────')}`);
        return;
    }

    // Multi-provider (debate, compare)
    if (data.perspectives) {
        console.log();
        console.log(`${purple('━━━')} ${bold('DEBATE')}: ${data.topic || ''} ${purple('━━━')}`);
        for (const [provider, info] of Object.entries(data.perspectives)) {
            console.log();
            console.log(`${cyan('┌')} ${bold(provider.toUpperCase())} ${dim(`[${info.stance}]`)}`);
            if (info.response) {
                info.response.split('\n').forEach(line => {
                    console.log(`${cyan('│')} ${line}`);
                });
            } else if (info.error) {
                console.log(`${cyan('│')} ${red('Error:')} ${info.error}`);
            }
            console.log(`${cyan('└──────────────────')}`);
        }
        return;
    }

    // All-provider response
    if (data.choices && data.choices.length > 1) {
        console.log();
        data.choices.forEach(choice => {
            const model = choice.model || `Response ${choice.index + 1}`;
            console.log(`${cyan('┌')} ${bold(model.toUpperCase())}${choice.responseTimeMs ? dim(` (${(choice.responseTimeMs/1000).toFixed(1)}s)`) : ''}`);
            (choice.message?.content || '').split('\n').forEach(line => {
                console.log(`${cyan('│')} ${line}`);
            });
            console.log(`${cyan('└──────────────────')}`);
            console.log();
        });
        return;
    }

    // Fallback
    if (data.error) {
        console.error(`${red('Error:')} ${data.error.message || JSON.stringify(data.error)}`);
        return;
    }

    console.log(JSON.stringify(data, null, 2));
}

// ─── Context Helpers ────────────────────────────────

function readStdin() {
    return new Promise((resolve) => {
        if (process.stdin.isTTY) { resolve(''); return; }
        let data = '';
        process.stdin.setEncoding('utf8');
        process.stdin.on('data', chunk => { data += chunk; });
        process.stdin.on('end', () => resolve(data.trim()));
        // Timeout — don't hang forever if no stdin
        setTimeout(() => resolve(data.trim()), 500);
    });
}

function readFileContext(filePath) {
    try {
        const resolved = path.resolve(filePath);
        if (!fs.existsSync(resolved)) return null;
        const stat = fs.statSync(resolved);
        if (stat.size > 500 * 1024) return `[File too large: ${(stat.size/1024).toFixed(0)}KB — max 500KB]`;
        const content = fs.readFileSync(resolved, 'utf8');
        const ext = path.extname(resolved).slice(1) || 'txt';
        return `\n\n--- File: ${path.basename(resolved)} ---\n\`\`\`${ext}\n${content}\n\`\`\``;
    } catch {
        return null;
    }
}

function buildMessage(userMessage, stdinContent, fileFlag) {
    let msg = userMessage || '';

    // Add stdin context (piped content)
    if (stdinContent) {
        msg = msg
            ? `${msg}\n\n--- Piped Context ---\n\`\`\`\n${stdinContent}\n\`\`\``
            : `Help me with this:\n\`\`\`\n${stdinContent}\n\`\`\``;
    }

    // Add file context
    if (fileFlag) {
        const files = Array.isArray(fileFlag) ? fileFlag : [fileFlag];
        for (const f of files) {
            const content = readFileContext(f);
            if (content) msg += content;
            else msg += `\n\n[File not found: ${f}]`;
        }
    }

    return msg;
}

// ─── Commands ───────────────────────────────────────

async function cmdAsk(model, message, context, filePath = null) {
    const fullMessage = context ? `${message}\n\n${context}` : message;
    const spinner = startSpinner(`Asking ${model}...`);
    try {
        const body = { model, message: fullMessage };
        if (filePath) {
            body.file = filePath;
        }
        const { data } = await apiRequest('POST', '/v1/chat/completions', body);
        spinner.stop(green('✓ Response received'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdSearch(query) {
    const spinner = startSpinner('Searching...');
    try {
        const { data } = await apiRequest('POST', '/v1/chat/completions', {
            model: 'perplexity', message: query, function: 'search'
        });
        spinner.stop(green('✓ Results found'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdTranslate(text, to, from) {
    const spinner = startSpinner(`Translating to ${to}...`);
    try {
        const body = { model: 'auto', message: text, function: 'translate', to };
        if (from) body.from = from;
        const { data } = await apiRequest('POST', '/v1/chat/completions', body);
        spinner.stop(green('✓ Translated'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdCode(action, description, language) {
    const spinner = startSpinner(`${action === 'generate' ? 'Generating' : action.charAt(0).toUpperCase() + action.slice(1) + 'ing'} code...`);
    try {
        const body = { model: 'claude', message: description, function: 'code', action };
        if (language) body.language = language;
        const { data } = await apiRequest('POST', '/v1/chat/completions', body);
        spinner.stop(green('✓ Done'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdDebate(topic) {
    const spinner = startSpinner('Debating across providers...');
    try {
        const { data } = await apiRequest('POST', '/v1/chat/completions', {
            model: 'all', message: topic, function: 'debate'
        });
        spinner.stop(green('✓ Debate complete'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdAudit(code) {
    const spinner = startSpinner('Running security audit...');
    try {
        const { data } = await apiRequest('POST', '/v1/chat/completions', {
            model: 'claude', code, function: 'security_audit'
        });
        spinner.stop(green('✓ Audit complete'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdBrainstorm(topic) {
    const spinner = startSpinner('Brainstorming ideas...');
    try {
        const { data } = await apiRequest('POST', '/v1/chat/completions', {
            model: 'auto', message: topic, function: 'brainstorm'
        });
        spinner.stop(green('✓ Ideas generated'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdAnalyze(urlOrContent, question) {
    const spinner = startSpinner('Analyzing...');
    try {
        const body = { model: 'perplexity', function: 'analyze' };
        if (urlOrContent.startsWith('http')) {
            body.url = urlOrContent;
        } else {
            body.message = urlOrContent;
        }
        if (question) body.question = question;
        const { data } = await apiRequest('POST', '/v1/chat/completions', body);
        spinner.stop(green('✓ Analysis complete'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdCompare(message) {
    const spinner = startSpinner('Querying all providers...');
    try {
        const { data } = await apiRequest('POST', '/v1/chat/completions', {
            model: 'all', message
        });
        spinner.stop(green('✓ All responses received'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdNew() {
    const spinner = startSpinner('Starting new conversations...');
    try {
        const { data } = await apiRequest('POST', '/v1/conversations/new');
        spinner.stop(green('✓ New conversations started'));
        console.log();
        console.log(`${green('✓')} All provider conversations reset.`);
        console.log();
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdFix(errorText, context) {
    const fullMessage = context
        ? `Fix this error. Here's the error and context:\n\nError:\n\`\`\`\n${errorText}\n\`\`\`\n\n${context}`
        : `Fix this error. Explain what went wrong and provide the fix:\n\n\`\`\`\n${errorText}\n\`\`\``;
    const spinner = startSpinner('Analyzing error...');
    try {
        const { data } = await apiRequest('POST', '/v1/chat/completions', {
            model: 'auto', message: fullMessage
        });
        spinner.stop(green('✓ Fix found'));
        formatResponse(data);
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdModels() {
    const spinner = startSpinner('Fetching models...');
    try {
        const { data } = await apiRequest('GET', '/v1/models');
        spinner.stop();
        console.log();
        console.log(`${bold('Available Models:')}`);
        console.log();
        (data.data || []).forEach(m => {
            const status = m.status === 'enabled' ? green('● ON ') : red('○ OFF');
            const aliases = m.aliases?.length ? dim(` (${m.aliases.join(', ')})`) : '';
            console.log(`  ${status} ${bold(m.id)}${aliases}`);
        });
        console.log();
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdStatus() {
    const spinner = startSpinner('Checking status...');
    try {
        const { data } = await apiRequest('GET', '/api/status');
        spinner.stop();
        console.log();
        console.log(`${purple('⚡')} ${bold('Synapse')} v${data.version}`);
        console.log(`${dim('   Port:')} ${data.port}`);
        console.log(`${dim('   Providers:')}`);
        (data.enabledProviders || []).forEach(p => {
            console.log(`     ${green('●')} ${p}`);
        });
        if (data.stats) {
            console.log(`${dim('   Requests:')} ${data.stats.totalRequests} (${data.stats.totalErrors} errors)`);
            console.log(`${dim('   Uptime:')} ${data.stats.uptime}`);
        }
        console.log();
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

async function cmdStats() {
    const spinner = startSpinner('Fetching stats...');
    try {
        const { data } = await apiRequest('GET', '/v1/stats');
        spinner.stop();
        console.log();
        console.log(`${bold('Provider Stats:')}`);
        console.log();
        for (const [name, info] of Object.entries(data.providers || {})) {
            console.log(`  ${cyan(bold(name))}`);
            console.log(`    Calls: ${info.calls}  Errors: ${info.errors}`);
            console.log(`    Avg: ${info.avgTime}  Min: ${info.minTime}  Max: ${info.maxTime}`);
            if (info.lastCall) console.log(`    Last: ${dim(info.lastCall)}`);
            console.log();
        }
    } catch (e) {
        spinner.stop(red('✗ Failed'));
        console.error(red(e.message));
    }
}

// ─── Help ───────────────────────────────────────────
function showHelp() {
    console.log(`
${purple('⚡')} ${bold('Synapse CLI')} ${dim(`v${version}`)}
${dim('   Unified AI Gateway — Talk to AI from your terminal')}

${yellow('USAGE:')}

  ${green('Chat:')}
    synapse ask ${dim('[model]')} "message"       Chat with an AI
    synapse ask claude "What is AI?"
    synapse ask chatgpt "Write a poem"
    synapse ask auto "Hello"              ${dim('(auto-pick best provider)')}

  ${green('Compare:')}
    synapse compare "What is AI?"          ${dim('Ask all providers, see side-by-side')}

  ${green('Search:')}
    synapse search "query"                Web search via Perplexity

  ${green('Brainstorm:')}
    synapse brainstorm "startup ideas"    ${dim('Generate creative ideas')}

  ${green('Translate:')}
    synapse translate "Hello" --to Hindi
    synapse translate "Bonjour" --to English --from French

  ${green('Code:')}
    synapse code "sort algorithm"         ${dim('Generate code')}
    synapse code review "def add(a,b)"    ${dim('Review code')}
    synapse code explain "async/await"    ${dim('Explain code')}

  ${green('Debate:')}
    synapse debate "Is AI dangerous?"     ${dim('Multi-provider debate')}

  ${green('Security:')}
    synapse audit "<code snippet>"        ${dim('Security vulnerability scan')}

  ${green('Analyze:')}
    synapse analyze "https://example.com" ${dim('Analyze a URL')}
    synapse analyze "some text" --q "summarize"  ${dim('Analyze content')}

  ${green('Fix Error:')}
    synapse fix "TypeError: x is not a function"   ${dim('Fix an error')}
    npm run build 2>&1 | synapse fix                ${dim('Pipe error output')}
    synapse fix "error" --file src/app.js           ${dim('Error + file context')}

  ${green('Context Flags:')}
    synapse ask "explain" --file src/app.js         ${dim('Send file as context')}
    cat log.txt | synapse ask "what went wrong?"    ${dim('Pipe any output')}
    git diff | synapse code review                  ${dim('Review git changes')}

  ${green('Session:')}
    synapse new                           ${dim('Reset all conversations')}

  ${green('Info:')}
    synapse models                        ${dim('List available models')}
    synapse status                        ${dim('Server status')}
    synapse stats                         ${dim('Provider statistics')}
    synapse help                          ${dim('Show this help')}

${yellow('OPTIONS:')}
    --model, -m    ${dim('Specify model (default: auto)')}
    --to           ${dim('Target language for translate')}
    --from         ${dim('Source language for translate')}
    --lang, -l     ${dim('Programming language for code')}
    --q            ${dim('Question for analyze')}
    --file         ${dim('Send file content as context')}
    --image        ${dim('Upload an image file (PNG/JPG)')}
    --pdf          ${dim('Upload a PDF file')}
    --json         ${dim('Raw JSON output (for scripts)')}

${yellow('ENV:')}
    SYNAPSE_HOST   ${dim('API host (default: 127.0.0.1)')}
    SYNAPSE_PORT   ${dim('API port (default: 3210)')}
`);
}

// ─── Arg Parser ─────────────────────────────────────
function parseArgs(argv) {
    const args = argv.slice(2);
    const result = { command: null, subcommand: null, positional: [], flags: {} };

    for (let i = 0; i < args.length; i++) {
        const arg = args[i];
        if (arg.startsWith('--')) {
            const key = arg.slice(2);
            const val = args[i + 1] && !args[i + 1].startsWith('-') ? args[++i] : true;
            result.flags[key] = val;
        } else if (arg.startsWith('-') && arg.length === 2) {
            const shortMap = { m: 'model', l: 'lang', t: 'to', f: 'from' };
            const key = shortMap[arg[1]] || arg[1];
            const val = args[i + 1] && !args[i + 1].startsWith('-') ? args[++i] : true;
            result.flags[key] = val;
        } else if (!result.command) {
            result.command = arg.toLowerCase();
        } else if (!result.subcommand && ['review', 'explain', 'debug', 'generate'].includes(arg.toLowerCase())) {
            result.subcommand = arg.toLowerCase();
        } else {
            result.positional.push(arg);
        }
    }

    return result;
}

// ─── Main ───────────────────────────────────────────
async function main() {
    const { command, subcommand, positional, flags } = parseArgs(process.argv);

    // Read stdin if piped
    const stdinContent = await readStdin();

    if (!command || command === 'help' || flags.help) {
        // If stdin has content but no command, treat as quick fix
        if (stdinContent) {
            await cmdFix(stdinContent);
            return;
        }
        showHelp();
        return;
    }

    // Build context from stdin + file flags
    const fileContext = flags.file ? readFileContext(flags.file) : '';
    const extraContext = [stdinContent, fileContext].filter(Boolean).join('\n\n');

    let uploadFilePath = null;
    if (flags.image && typeof flags.image === 'string') {
        const resolvedPath = path.resolve(flags.image);
        if (fs.existsSync(resolvedPath)) {
            uploadFilePath = resolvedPath;
        } else {
            console.error(red(`Error: Image file not found at ${flags.image}`));
            process.exit(1);
        }
    } else if (flags.pdf && typeof flags.pdf === 'string') {
        const resolvedPath = path.resolve(flags.pdf);
        if (fs.existsSync(resolvedPath)) {
            uploadFilePath = resolvedPath;
        } else {
            console.error(red(`Error: PDF file not found at ${flags.pdf}`));
            process.exit(1);
        }
    }

    switch (command) {
        case 'ask':
        case 'chat': {
            let model = 'auto';
            let message;
            if (positional.length >= 2) {
                model = positional[0];
                message = positional.slice(1).join(' ');
            } else if (positional.length === 1) {
                message = positional[0];
            }
            model = flags.model || model;
            if (!message && !stdinContent) { console.error(red('Usage: synapse ask [model] "message"')); return; }
            const fullMsg = buildMessage(message, stdinContent, flags.file);
            await cmdAsk(model, fullMsg, null, uploadFilePath);
            break;
        }

        case 'search': {
            const query = positional.join(' ');
            if (!query) { console.error(red('Usage: synapse search "query"')); return; }
            await cmdSearch(query);
            break;
        }

        case 'translate': {
            const text = positional.join(' ');
            const to = flags.to;
            if (!text || !to) { console.error(red('Usage: synapse translate "text" --to Language')); return; }
            await cmdTranslate(text, to, flags.from);
            break;
        }

        case 'code': {
            const action = subcommand || 'generate';
            const desc = positional.join(' ');
            const codeInput = stdinContent || desc;
            if (!codeInput) { console.error(red('Usage: synapse code [action] "description"')); return; }
            const codeMsg = stdinContent && desc
                ? `${desc}\n\n\`\`\`\n${stdinContent}\n\`\`\``
                : codeInput;
            await cmdCode(action, codeMsg, flags.lang || flags.language);
            break;
        }

        case 'debate': {
            const topic = positional.join(' ');
            if (!topic) { console.error(red('Usage: synapse debate "topic"')); return; }
            await cmdDebate(topic);
            break;
        }

        case 'audit':
        case 'security': {
            const code = stdinContent || positional.join(' ');
            if (!code) { console.error(red('Usage: synapse audit "code" or pipe: cat file.js | synapse audit')); return; }
            await cmdAudit(code);
            break;
        }

        case 'fix':
        case 'error': {
            const errorText = positional.join(' ') || stdinContent;
            if (!errorText) { console.error(red('Usage: synapse fix "error" or pipe: command 2>&1 | synapse fix')); return; }
            await cmdFix(errorText, fileContext);
            break;
        }

        case 'brainstorm':
        case 'ideas': {
            const topic = positional.join(' ');
            if (!topic) { console.error(red('Usage: synapse brainstorm "topic"')); return; }
            await cmdBrainstorm(topic);
            break;
        }

        case 'analyze': {
            const content = positional.join(' ');
            if (!content) { console.error(red('Usage: synapse analyze "url or text"')); return; }
            await cmdAnalyze(content, flags.q || flags.question);
            break;
        }

        case 'compare': {
            const msg = positional.join(' ');
            if (!msg) { console.error(red('Usage: synapse compare "question"')); return; }
            await cmdCompare(msg);
            break;
        }

        case 'new':
        case 'reset':
            await cmdNew();
            break;

        case 'models':
            await cmdModels();
            break;

        case 'status':
            await cmdStatus();
            break;

        case 'stats':
            await cmdStats();
            break;

        default:
            // Treat as a quick ask: synapse "What is AI?"
            const quickMessage = [command, ...positional].join(' ');
            await cmdAsk('auto', quickMessage, null, uploadFilePath);
            break;
    }
}

main().catch(e => {
    console.error(red(`Error: ${e.message}`));
    process.exit(1);
});
