// YuioLink client-side encryption (WebCrypto), wire-compatible with the Rust
// `yuiolink-core` crate: AES-256-GCM, 96-bit nonce, sealed string
// `yl1.<b64url(nonce)>.<b64url(ciphertext||tag)>`, key carried in the URL
// fragment as base64url. The key never leaves the browser.
window.YuioCrypto = (() => {
    "use strict";

    const b64urlEncode = (bytes) => {
        let bin = "";
        for (const b of bytes) bin += String.fromCharCode(b);
        return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    };

    const b64urlDecode = (str) => {
        const b64 = str.replace(/-/g, "+").replace(/_/g, "/");
        const padded = b64.padEnd(Math.ceil(b64.length / 4) * 4, "=");
        return Uint8Array.from(atob(padded), (c) => c.charCodeAt(0));
    };

    const generateKey = () => crypto.getRandomValues(new Uint8Array(32));

    const seal = async (plaintext, rawKey) => {
        const key = await crypto.subtle.importKey("raw", rawKey, "AES-GCM", false, ["encrypt"]);
        const iv = crypto.getRandomValues(new Uint8Array(12));
        const ct = await crypto.subtle.encrypt(
            { name: "AES-GCM", iv },
            key,
            new TextEncoder().encode(plaintext),
        );
        return `yl1.${b64urlEncode(iv)}.${b64urlEncode(new Uint8Array(ct))}`;
    };

    const open = async (sealed, rawKey) => {
        const parts = sealed.split(".");
        if (parts.length !== 3 || parts[0] !== "yl1") {
            throw new Error("unsupported sealed format");
        }
        const iv = b64urlDecode(parts[1]);
        const ct = b64urlDecode(parts[2]);
        const key = await crypto.subtle.importKey("raw", rawKey, "AES-GCM", false, ["decrypt"]);
        const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ct);
        return new TextDecoder().decode(pt);
    };

    return { generateKey, seal, open, keyToFragment: b64urlEncode, fragmentToKey: b64urlDecode };
})();
