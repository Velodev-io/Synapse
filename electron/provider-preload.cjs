const { webFrame } = require('electron');

const chromeVersion = '130.0.6723.191';
let userAgent = '';
let platform = '';
let uaPlatform = '';
let uaPlatformVersion = '';
let uaArch = '';

if (process.platform === 'darwin') {
    userAgent = `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${chromeVersion} Safari/537.36`;
    platform = 'MacIntel';
    uaPlatform = 'macOS';
    uaPlatformVersion = '14.5.0';
    uaArch = 'arm';
} else if (process.platform === 'linux') {
    userAgent = `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${chromeVersion} Safari/537.36`;
    platform = 'Linux x86_64';
    uaPlatform = 'Linux';
    uaPlatformVersion = 'unknown';
    uaArch = 'x86';
} else {
    userAgent = `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${chromeVersion} Safari/537.36`;
    platform = 'Win32';
    uaPlatform = 'Windows';
    uaPlatformVersion = '15.0.0';
    uaArch = 'x86';
}

const stealthScript = `
    (function() {
        'use strict';
        try {
            Object.defineProperty(navigator, 'webdriver', { get: () => false, configurable: true });

            const electronGlobals = ['process', 'require', 'module', '__filename', '__dirname'];
            electronGlobals.forEach(g => {
                try { delete window[g]; } catch(e) {}
                try { Object.defineProperty(window, g, { get: () => undefined, configurable: true }); } catch(e) {}
            });

            if (!window.chrome) window.chrome = {};
            if (!window.chrome.runtime) {
                window.chrome.runtime = {
                    OnInstalledReason: {},
                    OnRestartRequiredReason: {},
                    PlatformArch: { ARM: 'arm', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
                    PlatformNaclArch: { ARM: 'arm', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
                    PlatformOs: { ANDROID: 'android', CROS: 'cros', LINUX: 'linux', MAC: 'mac', OPENBSD: 'openbsd', WIN: 'win' },
                    RequestUpdateCheckStatus: { NO_UPDATE: 'no_update', THROTTLED: 'throttled', UPDATE_AVAILABLE: 'update_available' },
                    connect: function() { throw new Error('Could not establish connection. Receiving end does not exist.'); },
                    sendMessage: function() { throw new Error('Could not establish connection. Receiving end does not exist.'); },
                    id: undefined
                };
            }
            if (!window.chrome.app) window.chrome.app = { isInstalled: false, InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' }, RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' } };
            if (!window.chrome.csi) window.chrome.csi = function() { return { pageT: performance.now(), startE: Date.now(), onloadT: Date.now() }; };
            if (!window.chrome.loadTimes) window.chrome.loadTimes = function() { return { commitLoadTime: Date.now()/1000, connectionInfo: 'h2', finishDocumentLoadTime: Date.now()/1000, finishLoadTime: Date.now()/1000, firstPaintAfterLoadTime: 0, firstPaintTime: Date.now()/1000, navigationType: 'Other', npnNegotiatedProtocol: 'h2', requestTime: Date.now()/1000, startLoadTime: Date.now()/1000, wasAlternateProtocolAvailable: false, wasFetchedViaSpdy: true, wasNpnNegotiated: true }; };

            const navProps = {
                platform: ${JSON.stringify(platform)},
                vendor: 'Google Inc.',
                languages: ['en-US', 'en'],
                hardwareConcurrency: navigator.hardwareConcurrency || 8,
                deviceMemory: 8,
                maxTouchPoints: 0,
            };
            Object.entries(navProps).forEach(([key, val]) => {
                try { Object.defineProperty(navigator, key, { get: () => val, configurable: true }); } catch(e) {}
            });

            try {
                Object.defineProperty(navigator, 'plugins', {
                    get: () => {
                        const arr = [
                            { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
                            { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai', description: '' },
                            { name: 'Native Client', filename: 'internal-nacl-plugin', description: '' }
                        ];
                        arr.item = (i) => arr[i];
                        arr.namedItem = (name) => arr.find(p => p.name === name);
                        arr.refresh = () => {};
                        return arr;
                    },
                    configurable: true
                });
            } catch(e) {}

            try {
                const brands = [
                    { brand: "Chromium", version: "130" },
                    { brand: "Google Chrome", version: "130" },
                    { brand: "Not?A_Brand", version: "99" }
                ];
                const uad = {
                    brands,
                    mobile: false,
                    platform: ${JSON.stringify(uaPlatform)},
                    getHighEntropyValues: (hints) => Promise.resolve({
                        brands,
                        mobile: false,
                        platform: ${JSON.stringify(uaPlatform)},
                        platformVersion: ${JSON.stringify(uaPlatformVersion)},
                        architecture: ${JSON.stringify(uaArch)},
                        bitness: "64",
                        model: "",
                        uaFullVersion: ${JSON.stringify(chromeVersion)},
                        fullVersionList: [
                            { brand: "Chromium", version: ${JSON.stringify(chromeVersion)} },
                            { brand: "Google Chrome", version: ${JSON.stringify(chromeVersion)} },
                            { brand: "Not?A_Brand", version: "99.0.0.0" }
                        ],
                        wow64: false
                    }),
                    toJSON: function() { return { brands, mobile: false, platform: ${JSON.stringify(uaPlatform)} }; }
                };
                Object.defineProperty(navigator, 'userAgentData', { get: () => uad, configurable: true });
            } catch(e) {}

            try {
                const origQuery = window.Permissions.prototype.query;
                window.Permissions.prototype.query = function(params) {
                    if (params && params.name === 'notifications') {
                        return Promise.resolve({ state: Notification.permission });
                    }
                    return origQuery.call(this, params);
                };
            } catch(e) {}

            try {
                const getParam = WebGLRenderingContext.prototype.getParameter;
                WebGLRenderingContext.prototype.getParameter = function(param) {
                    if (param === 37445) return 'Google Inc. (NVIDIA)';
                    if (param === 37446) return 'ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11 vs_5_0 ps_5_0, D3D11)';
                    return getParam.call(this, param);
                };
                const getParam2 = WebGL2RenderingContext.prototype.getParameter;
                WebGL2RenderingContext.prototype.getParameter = function(param) {
                    if (param === 37445) return 'Google Inc. (NVIDIA)';
                    if (param === 37446) return 'ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11 vs_5_0 ps_5_0, D3D11)';
                    return getParam2.call(this, param);
                };
            } catch(e) {}

            try {
                const origContentWindow = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'contentWindow');
                Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
                    get: function() {
                        const win = origContentWindow.get.call(this);
                        if (win) {
                            try {
                                Object.defineProperty(win, 'chrome', { get: () => window.chrome, configurable: true });
                            } catch(e) {}
                        }
                        return win;
                    }
                });
            } catch(e) {}

            try {
                const screenProps = {
                    colorDepth: 24,
                    pixelDepth: 24,
                    availWidth: screen.availWidth || 1920,
                    availHeight: screen.availHeight || 1040,
                    width: screen.width || 1920,
                    height: screen.height || 1080,
                };
                Object.entries(screenProps).forEach(([key, val]) => {
                    try { Object.defineProperty(screen, key, { get: () => val, configurable: true }); } catch(e) {}
                });
            } catch(e) {}

            try {
                if (!window.outerWidth || window.outerWidth === 0) {
                    Object.defineProperty(window, 'outerWidth', { get: () => window.innerWidth || 1920, configurable: true });
                }
                if (!window.outerHeight || window.outerHeight === 0) {
                    Object.defineProperty(window, 'outerHeight', { get: () => (window.innerHeight || 1040) + 85, configurable: true });
                }
            } catch(e) {}

            try {
                if (typeof Notification !== 'undefined') {
                    const OrigNotification = Notification;
                    if (!OrigNotification.requestPermission) {
                        OrigNotification.requestPermission = function(cb) {
                            const p = Promise.resolve('default');
                            if (cb) p.then(cb);
                            return p;
                        };
                    }
                }
            } catch(e) {}

            console.log('[Compat] Preload stealth active');
        } catch(e) {
            console.log('[Compat] Preload stealth error:', e.message);
        }
    })();
`;

webFrame.executeJavaScript(stealthScript);
