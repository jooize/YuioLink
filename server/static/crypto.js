// YuioLink client-side encryption (WebCrypto), wire-compatible with the Rust
// `yuiolink-core` crate: AES-256-GCM, 96-bit nonce, sealed string
// `yl1.<b64url(nonce)>.<b64url(ciphertext||tag)>`, key carried in the URL
// fragment as base64url. The key never leaves the browser.
window.YuioCrypto = (function () {
    function b64urlEncode(bytes) {
        var bin = "";
        for (var i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
        return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    }

    function b64urlDecode(str) {
        str = str.replace(/-/g, "+").replace(/_/g, "/");
        while (str.length % 4) str += "=";
        var bin = atob(str);
        var out = new Uint8Array(bin.length);
        for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
        return out;
    }

    function generateKey() {
        return crypto.getRandomValues(new Uint8Array(32));
    }

    async function seal(plaintext, rawKey) {
        var key = await crypto.subtle.importKey("raw", rawKey, "AES-GCM", false, ["encrypt"]);
        var iv = crypto.getRandomValues(new Uint8Array(12));
        var ct = await crypto.subtle.encrypt(
            { name: "AES-GCM", iv: iv },
            key,
            new TextEncoder().encode(plaintext)
        );
        return "yl1." + b64urlEncode(iv) + "." + b64urlEncode(new Uint8Array(ct));
    }

    async function open(sealed, rawKey) {
        var parts = sealed.split(".");
        if (parts.length !== 3 || parts[0] !== "yl1") {
            throw new Error("unsupported sealed format");
        }
        var iv = b64urlDecode(parts[1]);
        var ct = b64urlDecode(parts[2]);
        var key = await crypto.subtle.importKey("raw", rawKey, "AES-GCM", false, ["decrypt"]);
        var pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv: iv }, key, ct);
        return new TextDecoder().decode(pt);
    }

    return {
        generateKey: generateKey,
        seal: seal,
        open: open,
        keyToFragment: b64urlEncode,
        fragmentToKey: b64urlDecode,
    };
})();
