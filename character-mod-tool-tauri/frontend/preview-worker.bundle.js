"use strict";
(() => {
  var __create = Object.create;
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getProtoOf = Object.getPrototypeOf;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __esm = (fn, res) => function __init() {
    return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
  };
  var __commonJS = (cb, mod) => function __require() {
    return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
  };
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
    // If the importer is in node compatibility mode or this is not an ESM
    // file that has been converted to a CommonJS file using a Babel-
    // compatible transform (i.e. "__esModule" has not been set), then set
    // "default" to the CommonJS "module.exports" for node compatibility.
    isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
    mod
  ));
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

  // build-tools/node_modules/base64-js/index.js
  var require_base64_js = __commonJS({
    "build-tools/node_modules/base64-js/index.js"(exports) {
      "use strict";
      init_globals();
      exports.byteLength = byteLength;
      exports.toByteArray = toByteArray;
      exports.fromByteArray = fromByteArray;
      var lookup = [];
      var revLookup = [];
      var Arr = typeof Uint8Array !== "undefined" ? Uint8Array : Array;
      var code = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
      for (i = 0, len = code.length; i < len; ++i) {
        lookup[i] = code[i];
        revLookup[code.charCodeAt(i)] = i;
      }
      var i;
      var len;
      revLookup["-".charCodeAt(0)] = 62;
      revLookup["_".charCodeAt(0)] = 63;
      function getLens(b64) {
        var len2 = b64.length;
        if (len2 % 4 > 0) {
          throw new Error("Invalid string. Length must be a multiple of 4");
        }
        var validLen = b64.indexOf("=");
        if (validLen === -1) validLen = len2;
        var placeHoldersLen = validLen === len2 ? 0 : 4 - validLen % 4;
        return [validLen, placeHoldersLen];
      }
      function byteLength(b64) {
        var lens = getLens(b64);
        var validLen = lens[0];
        var placeHoldersLen = lens[1];
        return (validLen + placeHoldersLen) * 3 / 4 - placeHoldersLen;
      }
      function _byteLength(b64, validLen, placeHoldersLen) {
        return (validLen + placeHoldersLen) * 3 / 4 - placeHoldersLen;
      }
      function toByteArray(b64) {
        var tmp;
        var lens = getLens(b64);
        var validLen = lens[0];
        var placeHoldersLen = lens[1];
        var arr = new Arr(_byteLength(b64, validLen, placeHoldersLen));
        var curByte = 0;
        var len2 = placeHoldersLen > 0 ? validLen - 4 : validLen;
        var i2;
        for (i2 = 0; i2 < len2; i2 += 4) {
          tmp = revLookup[b64.charCodeAt(i2)] << 18 | revLookup[b64.charCodeAt(i2 + 1)] << 12 | revLookup[b64.charCodeAt(i2 + 2)] << 6 | revLookup[b64.charCodeAt(i2 + 3)];
          arr[curByte++] = tmp >> 16 & 255;
          arr[curByte++] = tmp >> 8 & 255;
          arr[curByte++] = tmp & 255;
        }
        if (placeHoldersLen === 2) {
          tmp = revLookup[b64.charCodeAt(i2)] << 2 | revLookup[b64.charCodeAt(i2 + 1)] >> 4;
          arr[curByte++] = tmp & 255;
        }
        if (placeHoldersLen === 1) {
          tmp = revLookup[b64.charCodeAt(i2)] << 10 | revLookup[b64.charCodeAt(i2 + 1)] << 4 | revLookup[b64.charCodeAt(i2 + 2)] >> 2;
          arr[curByte++] = tmp >> 8 & 255;
          arr[curByte++] = tmp & 255;
        }
        return arr;
      }
      function tripletToBase64(num) {
        return lookup[num >> 18 & 63] + lookup[num >> 12 & 63] + lookup[num >> 6 & 63] + lookup[num & 63];
      }
      function encodeChunk(uint8, start, end) {
        var tmp;
        var output = [];
        for (var i2 = start; i2 < end; i2 += 3) {
          tmp = (uint8[i2] << 16 & 16711680) + (uint8[i2 + 1] << 8 & 65280) + (uint8[i2 + 2] & 255);
          output.push(tripletToBase64(tmp));
        }
        return output.join("");
      }
      function fromByteArray(uint8) {
        var tmp;
        var len2 = uint8.length;
        var extraBytes = len2 % 3;
        var parts = [];
        var maxChunkLength = 16383;
        for (var i2 = 0, len22 = len2 - extraBytes; i2 < len22; i2 += maxChunkLength) {
          parts.push(encodeChunk(uint8, i2, i2 + maxChunkLength > len22 ? len22 : i2 + maxChunkLength));
        }
        if (extraBytes === 1) {
          tmp = uint8[len2 - 1];
          parts.push(
            lookup[tmp >> 2] + lookup[tmp << 4 & 63] + "=="
          );
        } else if (extraBytes === 2) {
          tmp = (uint8[len2 - 2] << 8) + uint8[len2 - 1];
          parts.push(
            lookup[tmp >> 10] + lookup[tmp >> 4 & 63] + lookup[tmp << 2 & 63] + "="
          );
        }
        return parts.join("");
      }
    }
  });

  // build-tools/node_modules/ieee754/index.js
  var require_ieee754 = __commonJS({
    "build-tools/node_modules/ieee754/index.js"(exports) {
      init_globals();
      exports.read = function(buffer, offset, isLE, mLen, nBytes) {
        var e, m;
        var eLen = nBytes * 8 - mLen - 1;
        var eMax = (1 << eLen) - 1;
        var eBias = eMax >> 1;
        var nBits = -7;
        var i = isLE ? nBytes - 1 : 0;
        var d = isLE ? -1 : 1;
        var s = buffer[offset + i];
        i += d;
        e = s & (1 << -nBits) - 1;
        s >>= -nBits;
        nBits += eLen;
        for (; nBits > 0; e = e * 256 + buffer[offset + i], i += d, nBits -= 8) {
        }
        m = e & (1 << -nBits) - 1;
        e >>= -nBits;
        nBits += mLen;
        for (; nBits > 0; m = m * 256 + buffer[offset + i], i += d, nBits -= 8) {
        }
        if (e === 0) {
          e = 1 - eBias;
        } else if (e === eMax) {
          return m ? NaN : (s ? -1 : 1) * Infinity;
        } else {
          m = m + Math.pow(2, mLen);
          e = e - eBias;
        }
        return (s ? -1 : 1) * m * Math.pow(2, e - mLen);
      };
      exports.write = function(buffer, value, offset, isLE, mLen, nBytes) {
        var e, m, c;
        var eLen = nBytes * 8 - mLen - 1;
        var eMax = (1 << eLen) - 1;
        var eBias = eMax >> 1;
        var rt = mLen === 23 ? Math.pow(2, -24) - Math.pow(2, -77) : 0;
        var i = isLE ? 0 : nBytes - 1;
        var d = isLE ? 1 : -1;
        var s = value < 0 || value === 0 && 1 / value < 0 ? 1 : 0;
        value = Math.abs(value);
        if (isNaN(value) || value === Infinity) {
          m = isNaN(value) ? 1 : 0;
          e = eMax;
        } else {
          e = Math.floor(Math.log(value) / Math.LN2);
          if (value * (c = Math.pow(2, -e)) < 1) {
            e--;
            c *= 2;
          }
          if (e + eBias >= 1) {
            value += rt / c;
          } else {
            value += rt * Math.pow(2, 1 - eBias);
          }
          if (value * c >= 2) {
            e++;
            c /= 2;
          }
          if (e + eBias >= eMax) {
            m = 0;
            e = eMax;
          } else if (e + eBias >= 1) {
            m = (value * c - 1) * Math.pow(2, mLen);
            e = e + eBias;
          } else {
            m = value * Math.pow(2, eBias - 1) * Math.pow(2, mLen);
            e = 0;
          }
        }
        for (; mLen >= 8; buffer[offset + i] = m & 255, i += d, m /= 256, mLen -= 8) {
        }
        e = e << mLen | m;
        eLen += mLen;
        for (; eLen > 0; buffer[offset + i] = e & 255, i += d, e /= 256, eLen -= 8) {
        }
        buffer[offset + i - d] |= s * 128;
      };
    }
  });

  // build-tools/node_modules/buffer/index.js
  var require_buffer = __commonJS({
    "build-tools/node_modules/buffer/index.js"(exports) {
      "use strict";
      init_globals();
      var base64 = require_base64_js();
      var ieee754 = require_ieee754();
      var customInspectSymbol = typeof Symbol === "function" && typeof Symbol["for"] === "function" ? Symbol["for"]("nodejs.util.inspect.custom") : null;
      exports.Buffer = Buffer3;
      exports.SlowBuffer = SlowBuffer;
      exports.INSPECT_MAX_BYTES = 50;
      var K_MAX_LENGTH = 2147483647;
      exports.kMaxLength = K_MAX_LENGTH;
      Buffer3.TYPED_ARRAY_SUPPORT = typedArraySupport();
      if (!Buffer3.TYPED_ARRAY_SUPPORT && typeof console !== "undefined" && typeof console.error === "function") {
        console.error(
          "This browser lacks typed array (Uint8Array) support which is required by `buffer` v5.x. Use `buffer` v4.x if you require old browser support."
        );
      }
      function typedArraySupport() {
        try {
          const arr = new Uint8Array(1);
          const proto = { foo: function() {
            return 42;
          } };
          Object.setPrototypeOf(proto, Uint8Array.prototype);
          Object.setPrototypeOf(arr, proto);
          return arr.foo() === 42;
        } catch (e) {
          return false;
        }
      }
      Object.defineProperty(Buffer3.prototype, "parent", {
        enumerable: true,
        get: function() {
          if (!Buffer3.isBuffer(this)) return void 0;
          return this.buffer;
        }
      });
      Object.defineProperty(Buffer3.prototype, "offset", {
        enumerable: true,
        get: function() {
          if (!Buffer3.isBuffer(this)) return void 0;
          return this.byteOffset;
        }
      });
      function createBuffer(length) {
        if (length > K_MAX_LENGTH) {
          throw new RangeError('The value "' + length + '" is invalid for option "size"');
        }
        const buf = new Uint8Array(length);
        Object.setPrototypeOf(buf, Buffer3.prototype);
        return buf;
      }
      function Buffer3(arg, encodingOrOffset, length) {
        if (typeof arg === "number") {
          if (typeof encodingOrOffset === "string") {
            throw new TypeError(
              'The "string" argument must be of type string. Received type number'
            );
          }
          return allocUnsafe(arg);
        }
        return from(arg, encodingOrOffset, length);
      }
      Buffer3.poolSize = 8192;
      function from(value, encodingOrOffset, length) {
        if (typeof value === "string") {
          return fromString(value, encodingOrOffset);
        }
        if (ArrayBuffer.isView(value)) {
          return fromArrayView(value);
        }
        if (value == null) {
          throw new TypeError(
            "The first argument must be one of type string, Buffer, ArrayBuffer, Array, or Array-like Object. Received type " + typeof value
          );
        }
        if (isInstance(value, ArrayBuffer) || value && isInstance(value.buffer, ArrayBuffer)) {
          return fromArrayBuffer(value, encodingOrOffset, length);
        }
        if (typeof SharedArrayBuffer !== "undefined" && (isInstance(value, SharedArrayBuffer) || value && isInstance(value.buffer, SharedArrayBuffer))) {
          return fromArrayBuffer(value, encodingOrOffset, length);
        }
        if (typeof value === "number") {
          throw new TypeError(
            'The "value" argument must not be of type number. Received type number'
          );
        }
        const valueOf = value.valueOf && value.valueOf();
        if (valueOf != null && valueOf !== value) {
          return Buffer3.from(valueOf, encodingOrOffset, length);
        }
        const b = fromObject(value);
        if (b) return b;
        if (typeof Symbol !== "undefined" && Symbol.toPrimitive != null && typeof value[Symbol.toPrimitive] === "function") {
          return Buffer3.from(value[Symbol.toPrimitive]("string"), encodingOrOffset, length);
        }
        throw new TypeError(
          "The first argument must be one of type string, Buffer, ArrayBuffer, Array, or Array-like Object. Received type " + typeof value
        );
      }
      Buffer3.from = function(value, encodingOrOffset, length) {
        return from(value, encodingOrOffset, length);
      };
      Object.setPrototypeOf(Buffer3.prototype, Uint8Array.prototype);
      Object.setPrototypeOf(Buffer3, Uint8Array);
      function assertSize(size) {
        if (typeof size !== "number") {
          throw new TypeError('"size" argument must be of type number');
        } else if (size < 0) {
          throw new RangeError('The value "' + size + '" is invalid for option "size"');
        }
      }
      function alloc(size, fill, encoding) {
        assertSize(size);
        if (size <= 0) {
          return createBuffer(size);
        }
        if (fill !== void 0) {
          return typeof encoding === "string" ? createBuffer(size).fill(fill, encoding) : createBuffer(size).fill(fill);
        }
        return createBuffer(size);
      }
      Buffer3.alloc = function(size, fill, encoding) {
        return alloc(size, fill, encoding);
      };
      function allocUnsafe(size) {
        assertSize(size);
        return createBuffer(size < 0 ? 0 : checked(size) | 0);
      }
      Buffer3.allocUnsafe = function(size) {
        return allocUnsafe(size);
      };
      Buffer3.allocUnsafeSlow = function(size) {
        return allocUnsafe(size);
      };
      function fromString(string, encoding) {
        if (typeof encoding !== "string" || encoding === "") {
          encoding = "utf8";
        }
        if (!Buffer3.isEncoding(encoding)) {
          throw new TypeError("Unknown encoding: " + encoding);
        }
        const length = byteLength(string, encoding) | 0;
        let buf = createBuffer(length);
        const actual = buf.write(string, encoding);
        if (actual !== length) {
          buf = buf.slice(0, actual);
        }
        return buf;
      }
      function fromArrayLike(array) {
        const length = array.length < 0 ? 0 : checked(array.length) | 0;
        const buf = createBuffer(length);
        for (let i = 0; i < length; i += 1) {
          buf[i] = array[i] & 255;
        }
        return buf;
      }
      function fromArrayView(arrayView) {
        if (isInstance(arrayView, Uint8Array)) {
          const copy = new Uint8Array(arrayView);
          return fromArrayBuffer(copy.buffer, copy.byteOffset, copy.byteLength);
        }
        return fromArrayLike(arrayView);
      }
      function fromArrayBuffer(array, byteOffset, length) {
        if (byteOffset < 0 || array.byteLength < byteOffset) {
          throw new RangeError('"offset" is outside of buffer bounds');
        }
        if (array.byteLength < byteOffset + (length || 0)) {
          throw new RangeError('"length" is outside of buffer bounds');
        }
        let buf;
        if (byteOffset === void 0 && length === void 0) {
          buf = new Uint8Array(array);
        } else if (length === void 0) {
          buf = new Uint8Array(array, byteOffset);
        } else {
          buf = new Uint8Array(array, byteOffset, length);
        }
        Object.setPrototypeOf(buf, Buffer3.prototype);
        return buf;
      }
      function fromObject(obj) {
        if (Buffer3.isBuffer(obj)) {
          const len = checked(obj.length) | 0;
          const buf = createBuffer(len);
          if (buf.length === 0) {
            return buf;
          }
          obj.copy(buf, 0, 0, len);
          return buf;
        }
        if (obj.length !== void 0) {
          if (typeof obj.length !== "number" || numberIsNaN(obj.length)) {
            return createBuffer(0);
          }
          return fromArrayLike(obj);
        }
        if (obj.type === "Buffer" && Array.isArray(obj.data)) {
          return fromArrayLike(obj.data);
        }
      }
      function checked(length) {
        if (length >= K_MAX_LENGTH) {
          throw new RangeError("Attempt to allocate Buffer larger than maximum size: 0x" + K_MAX_LENGTH.toString(16) + " bytes");
        }
        return length | 0;
      }
      function SlowBuffer(length) {
        if (+length != length) {
          length = 0;
        }
        return Buffer3.alloc(+length);
      }
      Buffer3.isBuffer = function isBuffer(b) {
        return b != null && b._isBuffer === true && b !== Buffer3.prototype;
      };
      Buffer3.compare = function compare(a, b) {
        if (isInstance(a, Uint8Array)) a = Buffer3.from(a, a.offset, a.byteLength);
        if (isInstance(b, Uint8Array)) b = Buffer3.from(b, b.offset, b.byteLength);
        if (!Buffer3.isBuffer(a) || !Buffer3.isBuffer(b)) {
          throw new TypeError(
            'The "buf1", "buf2" arguments must be one of type Buffer or Uint8Array'
          );
        }
        if (a === b) return 0;
        let x = a.length;
        let y = b.length;
        for (let i = 0, len = Math.min(x, y); i < len; ++i) {
          if (a[i] !== b[i]) {
            x = a[i];
            y = b[i];
            break;
          }
        }
        if (x < y) return -1;
        if (y < x) return 1;
        return 0;
      };
      Buffer3.isEncoding = function isEncoding(encoding) {
        switch (String(encoding).toLowerCase()) {
          case "hex":
          case "utf8":
          case "utf-8":
          case "ascii":
          case "latin1":
          case "binary":
          case "base64":
          case "ucs2":
          case "ucs-2":
          case "utf16le":
          case "utf-16le":
            return true;
          default:
            return false;
        }
      };
      Buffer3.concat = function concat(list, length) {
        if (!Array.isArray(list)) {
          throw new TypeError('"list" argument must be an Array of Buffers');
        }
        if (list.length === 0) {
          return Buffer3.alloc(0);
        }
        let i;
        if (length === void 0) {
          length = 0;
          for (i = 0; i < list.length; ++i) {
            length += list[i].length;
          }
        }
        const buffer = Buffer3.allocUnsafe(length);
        let pos = 0;
        for (i = 0; i < list.length; ++i) {
          let buf = list[i];
          if (isInstance(buf, Uint8Array)) {
            if (pos + buf.length > buffer.length) {
              if (!Buffer3.isBuffer(buf)) buf = Buffer3.from(buf);
              buf.copy(buffer, pos);
            } else {
              Uint8Array.prototype.set.call(
                buffer,
                buf,
                pos
              );
            }
          } else if (!Buffer3.isBuffer(buf)) {
            throw new TypeError('"list" argument must be an Array of Buffers');
          } else {
            buf.copy(buffer, pos);
          }
          pos += buf.length;
        }
        return buffer;
      };
      function byteLength(string, encoding) {
        if (Buffer3.isBuffer(string)) {
          return string.length;
        }
        if (ArrayBuffer.isView(string) || isInstance(string, ArrayBuffer)) {
          return string.byteLength;
        }
        if (typeof string !== "string") {
          throw new TypeError(
            'The "string" argument must be one of type string, Buffer, or ArrayBuffer. Received type ' + typeof string
          );
        }
        const len = string.length;
        const mustMatch = arguments.length > 2 && arguments[2] === true;
        if (!mustMatch && len === 0) return 0;
        let loweredCase = false;
        for (; ; ) {
          switch (encoding) {
            case "ascii":
            case "latin1":
            case "binary":
              return len;
            case "utf8":
            case "utf-8":
              return utf8ToBytes(string).length;
            case "ucs2":
            case "ucs-2":
            case "utf16le":
            case "utf-16le":
              return len * 2;
            case "hex":
              return len >>> 1;
            case "base64":
              return base64ToBytes(string).length;
            default:
              if (loweredCase) {
                return mustMatch ? -1 : utf8ToBytes(string).length;
              }
              encoding = ("" + encoding).toLowerCase();
              loweredCase = true;
          }
        }
      }
      Buffer3.byteLength = byteLength;
      function slowToString(encoding, start, end) {
        let loweredCase = false;
        if (start === void 0 || start < 0) {
          start = 0;
        }
        if (start > this.length) {
          return "";
        }
        if (end === void 0 || end > this.length) {
          end = this.length;
        }
        if (end <= 0) {
          return "";
        }
        end >>>= 0;
        start >>>= 0;
        if (end <= start) {
          return "";
        }
        if (!encoding) encoding = "utf8";
        while (true) {
          switch (encoding) {
            case "hex":
              return hexSlice(this, start, end);
            case "utf8":
            case "utf-8":
              return utf8Slice(this, start, end);
            case "ascii":
              return asciiSlice(this, start, end);
            case "latin1":
            case "binary":
              return latin1Slice(this, start, end);
            case "base64":
              return base64Slice(this, start, end);
            case "ucs2":
            case "ucs-2":
            case "utf16le":
            case "utf-16le":
              return utf16leSlice(this, start, end);
            default:
              if (loweredCase) throw new TypeError("Unknown encoding: " + encoding);
              encoding = (encoding + "").toLowerCase();
              loweredCase = true;
          }
        }
      }
      Buffer3.prototype._isBuffer = true;
      function swap(b, n, m) {
        const i = b[n];
        b[n] = b[m];
        b[m] = i;
      }
      Buffer3.prototype.swap16 = function swap16() {
        const len = this.length;
        if (len % 2 !== 0) {
          throw new RangeError("Buffer size must be a multiple of 16-bits");
        }
        for (let i = 0; i < len; i += 2) {
          swap(this, i, i + 1);
        }
        return this;
      };
      Buffer3.prototype.swap32 = function swap32() {
        const len = this.length;
        if (len % 4 !== 0) {
          throw new RangeError("Buffer size must be a multiple of 32-bits");
        }
        for (let i = 0; i < len; i += 4) {
          swap(this, i, i + 3);
          swap(this, i + 1, i + 2);
        }
        return this;
      };
      Buffer3.prototype.swap64 = function swap64() {
        const len = this.length;
        if (len % 8 !== 0) {
          throw new RangeError("Buffer size must be a multiple of 64-bits");
        }
        for (let i = 0; i < len; i += 8) {
          swap(this, i, i + 7);
          swap(this, i + 1, i + 6);
          swap(this, i + 2, i + 5);
          swap(this, i + 3, i + 4);
        }
        return this;
      };
      Buffer3.prototype.toString = function toString() {
        const length = this.length;
        if (length === 0) return "";
        if (arguments.length === 0) return utf8Slice(this, 0, length);
        return slowToString.apply(this, arguments);
      };
      Buffer3.prototype.toLocaleString = Buffer3.prototype.toString;
      Buffer3.prototype.equals = function equals(b) {
        if (!Buffer3.isBuffer(b)) throw new TypeError("Argument must be a Buffer");
        if (this === b) return true;
        return Buffer3.compare(this, b) === 0;
      };
      Buffer3.prototype.inspect = function inspect() {
        let str = "";
        const max2 = exports.INSPECT_MAX_BYTES;
        str = this.toString("hex", 0, max2).replace(/(.{2})/g, "$1 ").trim();
        if (this.length > max2) str += " ... ";
        return "<Buffer " + str + ">";
      };
      if (customInspectSymbol) {
        Buffer3.prototype[customInspectSymbol] = Buffer3.prototype.inspect;
      }
      Buffer3.prototype.compare = function compare(target, start, end, thisStart, thisEnd) {
        if (isInstance(target, Uint8Array)) {
          target = Buffer3.from(target, target.offset, target.byteLength);
        }
        if (!Buffer3.isBuffer(target)) {
          throw new TypeError(
            'The "target" argument must be one of type Buffer or Uint8Array. Received type ' + typeof target
          );
        }
        if (start === void 0) {
          start = 0;
        }
        if (end === void 0) {
          end = target ? target.length : 0;
        }
        if (thisStart === void 0) {
          thisStart = 0;
        }
        if (thisEnd === void 0) {
          thisEnd = this.length;
        }
        if (start < 0 || end > target.length || thisStart < 0 || thisEnd > this.length) {
          throw new RangeError("out of range index");
        }
        if (thisStart >= thisEnd && start >= end) {
          return 0;
        }
        if (thisStart >= thisEnd) {
          return -1;
        }
        if (start >= end) {
          return 1;
        }
        start >>>= 0;
        end >>>= 0;
        thisStart >>>= 0;
        thisEnd >>>= 0;
        if (this === target) return 0;
        let x = thisEnd - thisStart;
        let y = end - start;
        const len = Math.min(x, y);
        const thisCopy = this.slice(thisStart, thisEnd);
        const targetCopy = target.slice(start, end);
        for (let i = 0; i < len; ++i) {
          if (thisCopy[i] !== targetCopy[i]) {
            x = thisCopy[i];
            y = targetCopy[i];
            break;
          }
        }
        if (x < y) return -1;
        if (y < x) return 1;
        return 0;
      };
      function bidirectionalIndexOf(buffer, val, byteOffset, encoding, dir) {
        if (buffer.length === 0) return -1;
        if (typeof byteOffset === "string") {
          encoding = byteOffset;
          byteOffset = 0;
        } else if (byteOffset > 2147483647) {
          byteOffset = 2147483647;
        } else if (byteOffset < -2147483648) {
          byteOffset = -2147483648;
        }
        byteOffset = +byteOffset;
        if (numberIsNaN(byteOffset)) {
          byteOffset = dir ? 0 : buffer.length - 1;
        }
        if (byteOffset < 0) byteOffset = buffer.length + byteOffset;
        if (byteOffset >= buffer.length) {
          if (dir) return -1;
          else byteOffset = buffer.length - 1;
        } else if (byteOffset < 0) {
          if (dir) byteOffset = 0;
          else return -1;
        }
        if (typeof val === "string") {
          val = Buffer3.from(val, encoding);
        }
        if (Buffer3.isBuffer(val)) {
          if (val.length === 0) {
            return -1;
          }
          return arrayIndexOf(buffer, val, byteOffset, encoding, dir);
        } else if (typeof val === "number") {
          val = val & 255;
          if (typeof Uint8Array.prototype.indexOf === "function") {
            if (dir) {
              return Uint8Array.prototype.indexOf.call(buffer, val, byteOffset);
            } else {
              return Uint8Array.prototype.lastIndexOf.call(buffer, val, byteOffset);
            }
          }
          return arrayIndexOf(buffer, [val], byteOffset, encoding, dir);
        }
        throw new TypeError("val must be string, number or Buffer");
      }
      function arrayIndexOf(arr, val, byteOffset, encoding, dir) {
        let indexSize = 1;
        let arrLength = arr.length;
        let valLength = val.length;
        if (encoding !== void 0) {
          encoding = String(encoding).toLowerCase();
          if (encoding === "ucs2" || encoding === "ucs-2" || encoding === "utf16le" || encoding === "utf-16le") {
            if (arr.length < 2 || val.length < 2) {
              return -1;
            }
            indexSize = 2;
            arrLength /= 2;
            valLength /= 2;
            byteOffset /= 2;
          }
        }
        function read(buf, i2) {
          if (indexSize === 1) {
            return buf[i2];
          } else {
            return buf.readUInt16BE(i2 * indexSize);
          }
        }
        let i;
        if (dir) {
          let foundIndex = -1;
          for (i = byteOffset; i < arrLength; i++) {
            if (read(arr, i) === read(val, foundIndex === -1 ? 0 : i - foundIndex)) {
              if (foundIndex === -1) foundIndex = i;
              if (i - foundIndex + 1 === valLength) return foundIndex * indexSize;
            } else {
              if (foundIndex !== -1) i -= i - foundIndex;
              foundIndex = -1;
            }
          }
        } else {
          if (byteOffset + valLength > arrLength) byteOffset = arrLength - valLength;
          for (i = byteOffset; i >= 0; i--) {
            let found = true;
            for (let j = 0; j < valLength; j++) {
              if (read(arr, i + j) !== read(val, j)) {
                found = false;
                break;
              }
            }
            if (found) return i;
          }
        }
        return -1;
      }
      Buffer3.prototype.includes = function includes(val, byteOffset, encoding) {
        return this.indexOf(val, byteOffset, encoding) !== -1;
      };
      Buffer3.prototype.indexOf = function indexOf(val, byteOffset, encoding) {
        return bidirectionalIndexOf(this, val, byteOffset, encoding, true);
      };
      Buffer3.prototype.lastIndexOf = function lastIndexOf(val, byteOffset, encoding) {
        return bidirectionalIndexOf(this, val, byteOffset, encoding, false);
      };
      function hexWrite(buf, string, offset, length) {
        offset = Number(offset) || 0;
        const remaining = buf.length - offset;
        if (!length) {
          length = remaining;
        } else {
          length = Number(length);
          if (length > remaining) {
            length = remaining;
          }
        }
        const strLen = string.length;
        if (length > strLen / 2) {
          length = strLen / 2;
        }
        let i;
        for (i = 0; i < length; ++i) {
          const parsed = parseInt(string.substr(i * 2, 2), 16);
          if (numberIsNaN(parsed)) return i;
          buf[offset + i] = parsed;
        }
        return i;
      }
      function utf8Write(buf, string, offset, length) {
        return blitBuffer(utf8ToBytes(string, buf.length - offset), buf, offset, length);
      }
      function asciiWrite(buf, string, offset, length) {
        return blitBuffer(asciiToBytes(string), buf, offset, length);
      }
      function base64Write(buf, string, offset, length) {
        return blitBuffer(base64ToBytes(string), buf, offset, length);
      }
      function ucs2Write(buf, string, offset, length) {
        return blitBuffer(utf16leToBytes(string, buf.length - offset), buf, offset, length);
      }
      Buffer3.prototype.write = function write(string, offset, length, encoding) {
        if (offset === void 0) {
          encoding = "utf8";
          length = this.length;
          offset = 0;
        } else if (length === void 0 && typeof offset === "string") {
          encoding = offset;
          length = this.length;
          offset = 0;
        } else if (isFinite(offset)) {
          offset = offset >>> 0;
          if (isFinite(length)) {
            length = length >>> 0;
            if (encoding === void 0) encoding = "utf8";
          } else {
            encoding = length;
            length = void 0;
          }
        } else {
          throw new Error(
            "Buffer.write(string, encoding, offset[, length]) is no longer supported"
          );
        }
        const remaining = this.length - offset;
        if (length === void 0 || length > remaining) length = remaining;
        if (string.length > 0 && (length < 0 || offset < 0) || offset > this.length) {
          throw new RangeError("Attempt to write outside buffer bounds");
        }
        if (!encoding) encoding = "utf8";
        let loweredCase = false;
        for (; ; ) {
          switch (encoding) {
            case "hex":
              return hexWrite(this, string, offset, length);
            case "utf8":
            case "utf-8":
              return utf8Write(this, string, offset, length);
            case "ascii":
            case "latin1":
            case "binary":
              return asciiWrite(this, string, offset, length);
            case "base64":
              return base64Write(this, string, offset, length);
            case "ucs2":
            case "ucs-2":
            case "utf16le":
            case "utf-16le":
              return ucs2Write(this, string, offset, length);
            default:
              if (loweredCase) throw new TypeError("Unknown encoding: " + encoding);
              encoding = ("" + encoding).toLowerCase();
              loweredCase = true;
          }
        }
      };
      Buffer3.prototype.toJSON = function toJSON() {
        return {
          type: "Buffer",
          data: Array.prototype.slice.call(this._arr || this, 0)
        };
      };
      function base64Slice(buf, start, end) {
        if (start === 0 && end === buf.length) {
          return base64.fromByteArray(buf);
        } else {
          return base64.fromByteArray(buf.slice(start, end));
        }
      }
      function utf8Slice(buf, start, end) {
        end = Math.min(buf.length, end);
        const res = [];
        let i = start;
        while (i < end) {
          const firstByte = buf[i];
          let codePoint = null;
          let bytesPerSequence = firstByte > 239 ? 4 : firstByte > 223 ? 3 : firstByte > 191 ? 2 : 1;
          if (i + bytesPerSequence <= end) {
            let secondByte, thirdByte, fourthByte, tempCodePoint;
            switch (bytesPerSequence) {
              case 1:
                if (firstByte < 128) {
                  codePoint = firstByte;
                }
                break;
              case 2:
                secondByte = buf[i + 1];
                if ((secondByte & 192) === 128) {
                  tempCodePoint = (firstByte & 31) << 6 | secondByte & 63;
                  if (tempCodePoint > 127) {
                    codePoint = tempCodePoint;
                  }
                }
                break;
              case 3:
                secondByte = buf[i + 1];
                thirdByte = buf[i + 2];
                if ((secondByte & 192) === 128 && (thirdByte & 192) === 128) {
                  tempCodePoint = (firstByte & 15) << 12 | (secondByte & 63) << 6 | thirdByte & 63;
                  if (tempCodePoint > 2047 && (tempCodePoint < 55296 || tempCodePoint > 57343)) {
                    codePoint = tempCodePoint;
                  }
                }
                break;
              case 4:
                secondByte = buf[i + 1];
                thirdByte = buf[i + 2];
                fourthByte = buf[i + 3];
                if ((secondByte & 192) === 128 && (thirdByte & 192) === 128 && (fourthByte & 192) === 128) {
                  tempCodePoint = (firstByte & 15) << 18 | (secondByte & 63) << 12 | (thirdByte & 63) << 6 | fourthByte & 63;
                  if (tempCodePoint > 65535 && tempCodePoint < 1114112) {
                    codePoint = tempCodePoint;
                  }
                }
            }
          }
          if (codePoint === null) {
            codePoint = 65533;
            bytesPerSequence = 1;
          } else if (codePoint > 65535) {
            codePoint -= 65536;
            res.push(codePoint >>> 10 & 1023 | 55296);
            codePoint = 56320 | codePoint & 1023;
          }
          res.push(codePoint);
          i += bytesPerSequence;
        }
        return decodeCodePointsArray(res);
      }
      var MAX_ARGUMENTS_LENGTH = 4096;
      function decodeCodePointsArray(codePoints) {
        const len = codePoints.length;
        if (len <= MAX_ARGUMENTS_LENGTH) {
          return String.fromCharCode.apply(String, codePoints);
        }
        let res = "";
        let i = 0;
        while (i < len) {
          res += String.fromCharCode.apply(
            String,
            codePoints.slice(i, i += MAX_ARGUMENTS_LENGTH)
          );
        }
        return res;
      }
      function asciiSlice(buf, start, end) {
        let ret = "";
        end = Math.min(buf.length, end);
        for (let i = start; i < end; ++i) {
          ret += String.fromCharCode(buf[i] & 127);
        }
        return ret;
      }
      function latin1Slice(buf, start, end) {
        let ret = "";
        end = Math.min(buf.length, end);
        for (let i = start; i < end; ++i) {
          ret += String.fromCharCode(buf[i]);
        }
        return ret;
      }
      function hexSlice(buf, start, end) {
        const len = buf.length;
        if (!start || start < 0) start = 0;
        if (!end || end < 0 || end > len) end = len;
        let out = "";
        for (let i = start; i < end; ++i) {
          out += hexSliceLookupTable[buf[i]];
        }
        return out;
      }
      function utf16leSlice(buf, start, end) {
        const bytes2 = buf.slice(start, end);
        let res = "";
        for (let i = 0; i < bytes2.length - 1; i += 2) {
          res += String.fromCharCode(bytes2[i] + bytes2[i + 1] * 256);
        }
        return res;
      }
      Buffer3.prototype.slice = function slice(start, end) {
        const len = this.length;
        start = ~~start;
        end = end === void 0 ? len : ~~end;
        if (start < 0) {
          start += len;
          if (start < 0) start = 0;
        } else if (start > len) {
          start = len;
        }
        if (end < 0) {
          end += len;
          if (end < 0) end = 0;
        } else if (end > len) {
          end = len;
        }
        if (end < start) end = start;
        const newBuf = this.subarray(start, end);
        Object.setPrototypeOf(newBuf, Buffer3.prototype);
        return newBuf;
      };
      function checkOffset(offset, ext, length) {
        if (offset % 1 !== 0 || offset < 0) throw new RangeError("offset is not uint");
        if (offset + ext > length) throw new RangeError("Trying to access beyond buffer length");
      }
      Buffer3.prototype.readUintLE = Buffer3.prototype.readUIntLE = function readUIntLE(offset, byteLength2, noAssert) {
        offset = offset >>> 0;
        byteLength2 = byteLength2 >>> 0;
        if (!noAssert) checkOffset(offset, byteLength2, this.length);
        let val = this[offset];
        let mul = 1;
        let i = 0;
        while (++i < byteLength2 && (mul *= 256)) {
          val += this[offset + i] * mul;
        }
        return val;
      };
      Buffer3.prototype.readUintBE = Buffer3.prototype.readUIntBE = function readUIntBE(offset, byteLength2, noAssert) {
        offset = offset >>> 0;
        byteLength2 = byteLength2 >>> 0;
        if (!noAssert) {
          checkOffset(offset, byteLength2, this.length);
        }
        let val = this[offset + --byteLength2];
        let mul = 1;
        while (byteLength2 > 0 && (mul *= 256)) {
          val += this[offset + --byteLength2] * mul;
        }
        return val;
      };
      Buffer3.prototype.readUint8 = Buffer3.prototype.readUInt8 = function readUInt8(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 1, this.length);
        return this[offset];
      };
      Buffer3.prototype.readUint16LE = Buffer3.prototype.readUInt16LE = function readUInt16LE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 2, this.length);
        return this[offset] | this[offset + 1] << 8;
      };
      Buffer3.prototype.readUint16BE = Buffer3.prototype.readUInt16BE = function readUInt16BE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 2, this.length);
        return this[offset] << 8 | this[offset + 1];
      };
      Buffer3.prototype.readUint32LE = Buffer3.prototype.readUInt32LE = function readUInt32LE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 4, this.length);
        return (this[offset] | this[offset + 1] << 8 | this[offset + 2] << 16) + this[offset + 3] * 16777216;
      };
      Buffer3.prototype.readUint32BE = Buffer3.prototype.readUInt32BE = function readUInt32BE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 4, this.length);
        return this[offset] * 16777216 + (this[offset + 1] << 16 | this[offset + 2] << 8 | this[offset + 3]);
      };
      Buffer3.prototype.readBigUInt64LE = defineBigIntMethod(function readBigUInt64LE(offset) {
        offset = offset >>> 0;
        validateNumber(offset, "offset");
        const first = this[offset];
        const last = this[offset + 7];
        if (first === void 0 || last === void 0) {
          boundsError(offset, this.length - 8);
        }
        const lo = first + this[++offset] * 2 ** 8 + this[++offset] * 2 ** 16 + this[++offset] * 2 ** 24;
        const hi = this[++offset] + this[++offset] * 2 ** 8 + this[++offset] * 2 ** 16 + last * 2 ** 24;
        return BigInt(lo) + (BigInt(hi) << BigInt(32));
      });
      Buffer3.prototype.readBigUInt64BE = defineBigIntMethod(function readBigUInt64BE(offset) {
        offset = offset >>> 0;
        validateNumber(offset, "offset");
        const first = this[offset];
        const last = this[offset + 7];
        if (first === void 0 || last === void 0) {
          boundsError(offset, this.length - 8);
        }
        const hi = first * 2 ** 24 + this[++offset] * 2 ** 16 + this[++offset] * 2 ** 8 + this[++offset];
        const lo = this[++offset] * 2 ** 24 + this[++offset] * 2 ** 16 + this[++offset] * 2 ** 8 + last;
        return (BigInt(hi) << BigInt(32)) + BigInt(lo);
      });
      Buffer3.prototype.readIntLE = function readIntLE(offset, byteLength2, noAssert) {
        offset = offset >>> 0;
        byteLength2 = byteLength2 >>> 0;
        if (!noAssert) checkOffset(offset, byteLength2, this.length);
        let val = this[offset];
        let mul = 1;
        let i = 0;
        while (++i < byteLength2 && (mul *= 256)) {
          val += this[offset + i] * mul;
        }
        mul *= 128;
        if (val >= mul) val -= Math.pow(2, 8 * byteLength2);
        return val;
      };
      Buffer3.prototype.readIntBE = function readIntBE(offset, byteLength2, noAssert) {
        offset = offset >>> 0;
        byteLength2 = byteLength2 >>> 0;
        if (!noAssert) checkOffset(offset, byteLength2, this.length);
        let i = byteLength2;
        let mul = 1;
        let val = this[offset + --i];
        while (i > 0 && (mul *= 256)) {
          val += this[offset + --i] * mul;
        }
        mul *= 128;
        if (val >= mul) val -= Math.pow(2, 8 * byteLength2);
        return val;
      };
      Buffer3.prototype.readInt8 = function readInt8(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 1, this.length);
        if (!(this[offset] & 128)) return this[offset];
        return (255 - this[offset] + 1) * -1;
      };
      Buffer3.prototype.readInt16LE = function readInt16LE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 2, this.length);
        const val = this[offset] | this[offset + 1] << 8;
        return val & 32768 ? val | 4294901760 : val;
      };
      Buffer3.prototype.readInt16BE = function readInt16BE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 2, this.length);
        const val = this[offset + 1] | this[offset] << 8;
        return val & 32768 ? val | 4294901760 : val;
      };
      Buffer3.prototype.readInt32LE = function readInt32LE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 4, this.length);
        return this[offset] | this[offset + 1] << 8 | this[offset + 2] << 16 | this[offset + 3] << 24;
      };
      Buffer3.prototype.readInt32BE = function readInt32BE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 4, this.length);
        return this[offset] << 24 | this[offset + 1] << 16 | this[offset + 2] << 8 | this[offset + 3];
      };
      Buffer3.prototype.readBigInt64LE = defineBigIntMethod(function readBigInt64LE(offset) {
        offset = offset >>> 0;
        validateNumber(offset, "offset");
        const first = this[offset];
        const last = this[offset + 7];
        if (first === void 0 || last === void 0) {
          boundsError(offset, this.length - 8);
        }
        const val = this[offset + 4] + this[offset + 5] * 2 ** 8 + this[offset + 6] * 2 ** 16 + (last << 24);
        return (BigInt(val) << BigInt(32)) + BigInt(first + this[++offset] * 2 ** 8 + this[++offset] * 2 ** 16 + this[++offset] * 2 ** 24);
      });
      Buffer3.prototype.readBigInt64BE = defineBigIntMethod(function readBigInt64BE(offset) {
        offset = offset >>> 0;
        validateNumber(offset, "offset");
        const first = this[offset];
        const last = this[offset + 7];
        if (first === void 0 || last === void 0) {
          boundsError(offset, this.length - 8);
        }
        const val = (first << 24) + // Overflow
        this[++offset] * 2 ** 16 + this[++offset] * 2 ** 8 + this[++offset];
        return (BigInt(val) << BigInt(32)) + BigInt(this[++offset] * 2 ** 24 + this[++offset] * 2 ** 16 + this[++offset] * 2 ** 8 + last);
      });
      Buffer3.prototype.readFloatLE = function readFloatLE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 4, this.length);
        return ieee754.read(this, offset, true, 23, 4);
      };
      Buffer3.prototype.readFloatBE = function readFloatBE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 4, this.length);
        return ieee754.read(this, offset, false, 23, 4);
      };
      Buffer3.prototype.readDoubleLE = function readDoubleLE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 8, this.length);
        return ieee754.read(this, offset, true, 52, 8);
      };
      Buffer3.prototype.readDoubleBE = function readDoubleBE(offset, noAssert) {
        offset = offset >>> 0;
        if (!noAssert) checkOffset(offset, 8, this.length);
        return ieee754.read(this, offset, false, 52, 8);
      };
      function checkInt(buf, value, offset, ext, max2, min) {
        if (!Buffer3.isBuffer(buf)) throw new TypeError('"buffer" argument must be a Buffer instance');
        if (value > max2 || value < min) throw new RangeError('"value" argument is out of bounds');
        if (offset + ext > buf.length) throw new RangeError("Index out of range");
      }
      Buffer3.prototype.writeUintLE = Buffer3.prototype.writeUIntLE = function writeUIntLE(value, offset, byteLength2, noAssert) {
        value = +value;
        offset = offset >>> 0;
        byteLength2 = byteLength2 >>> 0;
        if (!noAssert) {
          const maxBytes = Math.pow(2, 8 * byteLength2) - 1;
          checkInt(this, value, offset, byteLength2, maxBytes, 0);
        }
        let mul = 1;
        let i = 0;
        this[offset] = value & 255;
        while (++i < byteLength2 && (mul *= 256)) {
          this[offset + i] = value / mul & 255;
        }
        return offset + byteLength2;
      };
      Buffer3.prototype.writeUintBE = Buffer3.prototype.writeUIntBE = function writeUIntBE(value, offset, byteLength2, noAssert) {
        value = +value;
        offset = offset >>> 0;
        byteLength2 = byteLength2 >>> 0;
        if (!noAssert) {
          const maxBytes = Math.pow(2, 8 * byteLength2) - 1;
          checkInt(this, value, offset, byteLength2, maxBytes, 0);
        }
        let i = byteLength2 - 1;
        let mul = 1;
        this[offset + i] = value & 255;
        while (--i >= 0 && (mul *= 256)) {
          this[offset + i] = value / mul & 255;
        }
        return offset + byteLength2;
      };
      Buffer3.prototype.writeUint8 = Buffer3.prototype.writeUInt8 = function writeUInt8(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 1, 255, 0);
        this[offset] = value & 255;
        return offset + 1;
      };
      Buffer3.prototype.writeUint16LE = Buffer3.prototype.writeUInt16LE = function writeUInt16LE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 2, 65535, 0);
        this[offset] = value & 255;
        this[offset + 1] = value >>> 8;
        return offset + 2;
      };
      Buffer3.prototype.writeUint16BE = Buffer3.prototype.writeUInt16BE = function writeUInt16BE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 2, 65535, 0);
        this[offset] = value >>> 8;
        this[offset + 1] = value & 255;
        return offset + 2;
      };
      Buffer3.prototype.writeUint32LE = Buffer3.prototype.writeUInt32LE = function writeUInt32LE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 4, 4294967295, 0);
        this[offset + 3] = value >>> 24;
        this[offset + 2] = value >>> 16;
        this[offset + 1] = value >>> 8;
        this[offset] = value & 255;
        return offset + 4;
      };
      Buffer3.prototype.writeUint32BE = Buffer3.prototype.writeUInt32BE = function writeUInt32BE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 4, 4294967295, 0);
        this[offset] = value >>> 24;
        this[offset + 1] = value >>> 16;
        this[offset + 2] = value >>> 8;
        this[offset + 3] = value & 255;
        return offset + 4;
      };
      function wrtBigUInt64LE(buf, value, offset, min, max2) {
        checkIntBI(value, min, max2, buf, offset, 7);
        let lo = Number(value & BigInt(4294967295));
        buf[offset++] = lo;
        lo = lo >> 8;
        buf[offset++] = lo;
        lo = lo >> 8;
        buf[offset++] = lo;
        lo = lo >> 8;
        buf[offset++] = lo;
        let hi = Number(value >> BigInt(32) & BigInt(4294967295));
        buf[offset++] = hi;
        hi = hi >> 8;
        buf[offset++] = hi;
        hi = hi >> 8;
        buf[offset++] = hi;
        hi = hi >> 8;
        buf[offset++] = hi;
        return offset;
      }
      function wrtBigUInt64BE(buf, value, offset, min, max2) {
        checkIntBI(value, min, max2, buf, offset, 7);
        let lo = Number(value & BigInt(4294967295));
        buf[offset + 7] = lo;
        lo = lo >> 8;
        buf[offset + 6] = lo;
        lo = lo >> 8;
        buf[offset + 5] = lo;
        lo = lo >> 8;
        buf[offset + 4] = lo;
        let hi = Number(value >> BigInt(32) & BigInt(4294967295));
        buf[offset + 3] = hi;
        hi = hi >> 8;
        buf[offset + 2] = hi;
        hi = hi >> 8;
        buf[offset + 1] = hi;
        hi = hi >> 8;
        buf[offset] = hi;
        return offset + 8;
      }
      Buffer3.prototype.writeBigUInt64LE = defineBigIntMethod(function writeBigUInt64LE(value, offset = 0) {
        return wrtBigUInt64LE(this, value, offset, BigInt(0), BigInt("0xffffffffffffffff"));
      });
      Buffer3.prototype.writeBigUInt64BE = defineBigIntMethod(function writeBigUInt64BE(value, offset = 0) {
        return wrtBigUInt64BE(this, value, offset, BigInt(0), BigInt("0xffffffffffffffff"));
      });
      Buffer3.prototype.writeIntLE = function writeIntLE(value, offset, byteLength2, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) {
          const limit = Math.pow(2, 8 * byteLength2 - 1);
          checkInt(this, value, offset, byteLength2, limit - 1, -limit);
        }
        let i = 0;
        let mul = 1;
        let sub = 0;
        this[offset] = value & 255;
        while (++i < byteLength2 && (mul *= 256)) {
          if (value < 0 && sub === 0 && this[offset + i - 1] !== 0) {
            sub = 1;
          }
          this[offset + i] = (value / mul >> 0) - sub & 255;
        }
        return offset + byteLength2;
      };
      Buffer3.prototype.writeIntBE = function writeIntBE(value, offset, byteLength2, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) {
          const limit = Math.pow(2, 8 * byteLength2 - 1);
          checkInt(this, value, offset, byteLength2, limit - 1, -limit);
        }
        let i = byteLength2 - 1;
        let mul = 1;
        let sub = 0;
        this[offset + i] = value & 255;
        while (--i >= 0 && (mul *= 256)) {
          if (value < 0 && sub === 0 && this[offset + i + 1] !== 0) {
            sub = 1;
          }
          this[offset + i] = (value / mul >> 0) - sub & 255;
        }
        return offset + byteLength2;
      };
      Buffer3.prototype.writeInt8 = function writeInt8(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 1, 127, -128);
        if (value < 0) value = 255 + value + 1;
        this[offset] = value & 255;
        return offset + 1;
      };
      Buffer3.prototype.writeInt16LE = function writeInt16LE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 2, 32767, -32768);
        this[offset] = value & 255;
        this[offset + 1] = value >>> 8;
        return offset + 2;
      };
      Buffer3.prototype.writeInt16BE = function writeInt16BE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 2, 32767, -32768);
        this[offset] = value >>> 8;
        this[offset + 1] = value & 255;
        return offset + 2;
      };
      Buffer3.prototype.writeInt32LE = function writeInt32LE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 4, 2147483647, -2147483648);
        this[offset] = value & 255;
        this[offset + 1] = value >>> 8;
        this[offset + 2] = value >>> 16;
        this[offset + 3] = value >>> 24;
        return offset + 4;
      };
      Buffer3.prototype.writeInt32BE = function writeInt32BE(value, offset, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) checkInt(this, value, offset, 4, 2147483647, -2147483648);
        if (value < 0) value = 4294967295 + value + 1;
        this[offset] = value >>> 24;
        this[offset + 1] = value >>> 16;
        this[offset + 2] = value >>> 8;
        this[offset + 3] = value & 255;
        return offset + 4;
      };
      Buffer3.prototype.writeBigInt64LE = defineBigIntMethod(function writeBigInt64LE(value, offset = 0) {
        return wrtBigUInt64LE(this, value, offset, -BigInt("0x8000000000000000"), BigInt("0x7fffffffffffffff"));
      });
      Buffer3.prototype.writeBigInt64BE = defineBigIntMethod(function writeBigInt64BE(value, offset = 0) {
        return wrtBigUInt64BE(this, value, offset, -BigInt("0x8000000000000000"), BigInt("0x7fffffffffffffff"));
      });
      function checkIEEE754(buf, value, offset, ext, max2, min) {
        if (offset + ext > buf.length) throw new RangeError("Index out of range");
        if (offset < 0) throw new RangeError("Index out of range");
      }
      function writeFloat(buf, value, offset, littleEndian, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) {
          checkIEEE754(buf, value, offset, 4, 34028234663852886e22, -34028234663852886e22);
        }
        ieee754.write(buf, value, offset, littleEndian, 23, 4);
        return offset + 4;
      }
      Buffer3.prototype.writeFloatLE = function writeFloatLE(value, offset, noAssert) {
        return writeFloat(this, value, offset, true, noAssert);
      };
      Buffer3.prototype.writeFloatBE = function writeFloatBE(value, offset, noAssert) {
        return writeFloat(this, value, offset, false, noAssert);
      };
      function writeDouble(buf, value, offset, littleEndian, noAssert) {
        value = +value;
        offset = offset >>> 0;
        if (!noAssert) {
          checkIEEE754(buf, value, offset, 8, 17976931348623157e292, -17976931348623157e292);
        }
        ieee754.write(buf, value, offset, littleEndian, 52, 8);
        return offset + 8;
      }
      Buffer3.prototype.writeDoubleLE = function writeDoubleLE(value, offset, noAssert) {
        return writeDouble(this, value, offset, true, noAssert);
      };
      Buffer3.prototype.writeDoubleBE = function writeDoubleBE(value, offset, noAssert) {
        return writeDouble(this, value, offset, false, noAssert);
      };
      Buffer3.prototype.copy = function copy(target, targetStart, start, end) {
        if (!Buffer3.isBuffer(target)) throw new TypeError("argument should be a Buffer");
        if (!start) start = 0;
        if (!end && end !== 0) end = this.length;
        if (targetStart >= target.length) targetStart = target.length;
        if (!targetStart) targetStart = 0;
        if (end > 0 && end < start) end = start;
        if (end === start) return 0;
        if (target.length === 0 || this.length === 0) return 0;
        if (targetStart < 0) {
          throw new RangeError("targetStart out of bounds");
        }
        if (start < 0 || start >= this.length) throw new RangeError("Index out of range");
        if (end < 0) throw new RangeError("sourceEnd out of bounds");
        if (end > this.length) end = this.length;
        if (target.length - targetStart < end - start) {
          end = target.length - targetStart + start;
        }
        const len = end - start;
        if (this === target && typeof Uint8Array.prototype.copyWithin === "function") {
          this.copyWithin(targetStart, start, end);
        } else {
          Uint8Array.prototype.set.call(
            target,
            this.subarray(start, end),
            targetStart
          );
        }
        return len;
      };
      Buffer3.prototype.fill = function fill(val, start, end, encoding) {
        if (typeof val === "string") {
          if (typeof start === "string") {
            encoding = start;
            start = 0;
            end = this.length;
          } else if (typeof end === "string") {
            encoding = end;
            end = this.length;
          }
          if (encoding !== void 0 && typeof encoding !== "string") {
            throw new TypeError("encoding must be a string");
          }
          if (typeof encoding === "string" && !Buffer3.isEncoding(encoding)) {
            throw new TypeError("Unknown encoding: " + encoding);
          }
          if (val.length === 1) {
            const code = val.charCodeAt(0);
            if (encoding === "utf8" && code < 128 || encoding === "latin1") {
              val = code;
            }
          }
        } else if (typeof val === "number") {
          val = val & 255;
        } else if (typeof val === "boolean") {
          val = Number(val);
        }
        if (start < 0 || this.length < start || this.length < end) {
          throw new RangeError("Out of range index");
        }
        if (end <= start) {
          return this;
        }
        start = start >>> 0;
        end = end === void 0 ? this.length : end >>> 0;
        if (!val) val = 0;
        let i;
        if (typeof val === "number") {
          for (i = start; i < end; ++i) {
            this[i] = val;
          }
        } else {
          const bytes2 = Buffer3.isBuffer(val) ? val : Buffer3.from(val, encoding);
          const len = bytes2.length;
          if (len === 0) {
            throw new TypeError('The value "' + val + '" is invalid for argument "value"');
          }
          for (i = 0; i < end - start; ++i) {
            this[i + start] = bytes2[i % len];
          }
        }
        return this;
      };
      var errors = {};
      function E(sym, getMessage, Base) {
        errors[sym] = class NodeError extends Base {
          constructor() {
            super();
            Object.defineProperty(this, "message", {
              value: getMessage.apply(this, arguments),
              writable: true,
              configurable: true
            });
            this.name = `${this.name} [${sym}]`;
            this.stack;
            delete this.name;
          }
          get code() {
            return sym;
          }
          set code(value) {
            Object.defineProperty(this, "code", {
              configurable: true,
              enumerable: true,
              value,
              writable: true
            });
          }
          toString() {
            return `${this.name} [${sym}]: ${this.message}`;
          }
        };
      }
      E(
        "ERR_BUFFER_OUT_OF_BOUNDS",
        function(name) {
          if (name) {
            return `${name} is outside of buffer bounds`;
          }
          return "Attempt to access memory outside buffer bounds";
        },
        RangeError
      );
      E(
        "ERR_INVALID_ARG_TYPE",
        function(name, actual) {
          return `The "${name}" argument must be of type number. Received type ${typeof actual}`;
        },
        TypeError
      );
      E(
        "ERR_OUT_OF_RANGE",
        function(str, range, input) {
          let msg = `The value of "${str}" is out of range.`;
          let received = input;
          if (Number.isInteger(input) && Math.abs(input) > 2 ** 32) {
            received = addNumericalSeparator(String(input));
          } else if (typeof input === "bigint") {
            received = String(input);
            if (input > BigInt(2) ** BigInt(32) || input < -(BigInt(2) ** BigInt(32))) {
              received = addNumericalSeparator(received);
            }
            received += "n";
          }
          msg += ` It must be ${range}. Received ${received}`;
          return msg;
        },
        RangeError
      );
      function addNumericalSeparator(val) {
        let res = "";
        let i = val.length;
        const start = val[0] === "-" ? 1 : 0;
        for (; i >= start + 4; i -= 3) {
          res = `_${val.slice(i - 3, i)}${res}`;
        }
        return `${val.slice(0, i)}${res}`;
      }
      function checkBounds(buf, offset, byteLength2) {
        validateNumber(offset, "offset");
        if (buf[offset] === void 0 || buf[offset + byteLength2] === void 0) {
          boundsError(offset, buf.length - (byteLength2 + 1));
        }
      }
      function checkIntBI(value, min, max2, buf, offset, byteLength2) {
        if (value > max2 || value < min) {
          const n = typeof min === "bigint" ? "n" : "";
          let range;
          if (byteLength2 > 3) {
            if (min === 0 || min === BigInt(0)) {
              range = `>= 0${n} and < 2${n} ** ${(byteLength2 + 1) * 8}${n}`;
            } else {
              range = `>= -(2${n} ** ${(byteLength2 + 1) * 8 - 1}${n}) and < 2 ** ${(byteLength2 + 1) * 8 - 1}${n}`;
            }
          } else {
            range = `>= ${min}${n} and <= ${max2}${n}`;
          }
          throw new errors.ERR_OUT_OF_RANGE("value", range, value);
        }
        checkBounds(buf, offset, byteLength2);
      }
      function validateNumber(value, name) {
        if (typeof value !== "number") {
          throw new errors.ERR_INVALID_ARG_TYPE(name, "number", value);
        }
      }
      function boundsError(value, length, type) {
        if (Math.floor(value) !== value) {
          validateNumber(value, type);
          throw new errors.ERR_OUT_OF_RANGE(type || "offset", "an integer", value);
        }
        if (length < 0) {
          throw new errors.ERR_BUFFER_OUT_OF_BOUNDS();
        }
        throw new errors.ERR_OUT_OF_RANGE(
          type || "offset",
          `>= ${type ? 1 : 0} and <= ${length}`,
          value
        );
      }
      var INVALID_BASE64_RE = /[^+/0-9A-Za-z-_]/g;
      function base64clean(str) {
        str = str.split("=")[0];
        str = str.trim().replace(INVALID_BASE64_RE, "");
        if (str.length < 2) return "";
        while (str.length % 4 !== 0) {
          str = str + "=";
        }
        return str;
      }
      function utf8ToBytes(string, units) {
        units = units || Infinity;
        let codePoint;
        const length = string.length;
        let leadSurrogate = null;
        const bytes2 = [];
        for (let i = 0; i < length; ++i) {
          codePoint = string.charCodeAt(i);
          if (codePoint > 55295 && codePoint < 57344) {
            if (!leadSurrogate) {
              if (codePoint > 56319) {
                if ((units -= 3) > -1) bytes2.push(239, 191, 189);
                continue;
              } else if (i + 1 === length) {
                if ((units -= 3) > -1) bytes2.push(239, 191, 189);
                continue;
              }
              leadSurrogate = codePoint;
              continue;
            }
            if (codePoint < 56320) {
              if ((units -= 3) > -1) bytes2.push(239, 191, 189);
              leadSurrogate = codePoint;
              continue;
            }
            codePoint = (leadSurrogate - 55296 << 10 | codePoint - 56320) + 65536;
          } else if (leadSurrogate) {
            if ((units -= 3) > -1) bytes2.push(239, 191, 189);
          }
          leadSurrogate = null;
          if (codePoint < 128) {
            if ((units -= 1) < 0) break;
            bytes2.push(codePoint);
          } else if (codePoint < 2048) {
            if ((units -= 2) < 0) break;
            bytes2.push(
              codePoint >> 6 | 192,
              codePoint & 63 | 128
            );
          } else if (codePoint < 65536) {
            if ((units -= 3) < 0) break;
            bytes2.push(
              codePoint >> 12 | 224,
              codePoint >> 6 & 63 | 128,
              codePoint & 63 | 128
            );
          } else if (codePoint < 1114112) {
            if ((units -= 4) < 0) break;
            bytes2.push(
              codePoint >> 18 | 240,
              codePoint >> 12 & 63 | 128,
              codePoint >> 6 & 63 | 128,
              codePoint & 63 | 128
            );
          } else {
            throw new Error("Invalid code point");
          }
        }
        return bytes2;
      }
      function asciiToBytes(str) {
        const byteArray = [];
        for (let i = 0; i < str.length; ++i) {
          byteArray.push(str.charCodeAt(i) & 255);
        }
        return byteArray;
      }
      function utf16leToBytes(str, units) {
        let c, hi, lo;
        const byteArray = [];
        for (let i = 0; i < str.length; ++i) {
          if ((units -= 2) < 0) break;
          c = str.charCodeAt(i);
          hi = c >> 8;
          lo = c % 256;
          byteArray.push(lo);
          byteArray.push(hi);
        }
        return byteArray;
      }
      function base64ToBytes(str) {
        return base64.toByteArray(base64clean(str));
      }
      function blitBuffer(src, dst, offset, length) {
        let i;
        for (i = 0; i < length; ++i) {
          if (i + offset >= dst.length || i >= src.length) break;
          dst[i + offset] = src[i];
        }
        return i;
      }
      function isInstance(obj, type) {
        return obj instanceof type || obj != null && obj.constructor != null && obj.constructor.name != null && obj.constructor.name === type.name;
      }
      function numberIsNaN(obj) {
        return obj !== obj;
      }
      var hexSliceLookupTable = (function() {
        const alphabet = "0123456789abcdef";
        const table = new Array(256);
        for (let i = 0; i < 16; ++i) {
          const i16 = i * 16;
          for (let j = 0; j < 16; ++j) {
            table[i16 + j] = alphabet[i] + alphabet[j];
          }
        }
        return table;
      })();
      function defineBigIntMethod(fn) {
        return typeof BigInt === "undefined" ? BufferBigIntNotDefined : fn;
      }
      function BufferBigIntNotDefined() {
        throw new Error("BigInt not supported");
      }
    }
  });

  // worker/globals.js
  var import_buffer;
  var init_globals = __esm({
    "worker/globals.js"() {
      "use strict";
      import_buffer = __toESM(require_buffer());
      globalThis.Buffer = import_buffer.Buffer;
      globalThis.process = { platform: "win32" };
    }
  });

  // worker/packet.mjs
  function geometryPart(metadata, buffer, start, size) {
    if (!metadata || typeof metadata !== "object" || Array.isArray(metadata) || !Array.isArray(metadata.meshes) || metadata.meshes.length > 1e3) throw new Error("Invalid binary geometry metadata.");
    const source = new Uint8Array(buffer, start, size), arrays = [];
    let offset = 0;
    for (const mesh of metadata.meshes) {
      if (!mesh || typeof mesh !== "object" || Array.isArray(mesh)) throw new Error("Invalid binary geometry mesh.");
      for (const key of GEOMETRY_KEYS) {
        const row = mesh[key];
        if (row == null) continue;
        const kind = key === "skinBones" ? "u16" : UINT_KEYS.has(key) ? "u32" : "f32";
        const ArrayType = kind === "f32" ? Float32Array : kind === "u32" ? Uint32Array : Uint16Array;
        if (typeof row !== "object" || row.kind !== kind || !Number.isSafeInteger(row.offset) || !Number.isSafeInteger(row.size) || row.offset !== offset || row.size < 0 || row.size % ArrayType.BYTES_PER_ELEMENT || row.size > size - offset || "data" in row) {
          throw new Error("Invalid binary geometry array range or type.");
        }
        const end = offset + row.size, next = Math.ceil(end / 4) * 4;
        if (next > size) throw new Error("Truncated binary geometry padding.");
        for (let index = end; index < next; index++) if (source[index] !== 0) throw new Error("Invalid binary geometry padding.");
        arrays.push({ mesh, key, ArrayType, offset, length: row.size / ArrayType.BYTES_PER_ELEMENT });
        offset = next;
      }
    }
    if (offset !== size) throw new Error("Trailing binary geometry data.");
    const owned = buffer.slice(start, start + size);
    for (const row of arrays) row.mesh[row.key] = new row.ArrayType(owned, row.offset, row.length);
    return metadata;
  }
  function unpackPreviewPacket(buffer) {
    if (!(buffer instanceof ArrayBuffer) || buffer.byteLength < 12 || buffer.byteLength > BODY_LIMIT + HEADER_LIMIT + 12) {
      throw new Error("Invalid binary preview packet size.");
    }
    const bytes2 = new Uint8Array(buffer);
    if (String.fromCharCode(...bytes2.subarray(0, 8)) !== "CMTBIN01") throw new Error("Unknown binary preview packet version.");
    const length = new DataView(buffer).getUint32(8, true);
    if (length > HEADER_LIMIT || length > bytes2.length - 12) throw new Error("Truncated binary preview metadata.");
    const metadata = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes2.subarray(12, 12 + length)));
    if (!metadata || typeof metadata !== "object" || Array.isArray(metadata)) throw new Error("Invalid binary preview metadata.");
    const start = 12 + length, size = bytes2.length - start;
    if (size > BODY_LIMIT) throw new Error("Binary preview data exceeds budget.");
    if (metadata.packetKind === "file") {
      if (metadata.size !== size) throw new Error("Binary preview file size differs.");
      return { data: bytes2.subarray(start) };
    }
    if (metadata.packetKind === "catalog-geometry") {
      if (!Number.isSafeInteger(metadata.assetBytes) || !Number.isSafeInteger(metadata.baseBytes) || metadata.assetBytes < 0 || metadata.baseBytes < 0 || metadata.assetBytes % 4 || metadata.baseBytes % 4 || metadata.assetBytes > size || metadata.baseBytes !== size - metadata.assetBytes || metadata.base == null && metadata.baseBytes !== 0 || !metadata.stats || typeof metadata.stats !== "object" || Array.isArray(metadata.stats)) {
        throw new Error("Invalid binary geometry segment sizes.");
      }
      return {
        asset: geometryPart(metadata.asset, buffer, start, metadata.assetBytes),
        base: metadata.base == null ? null : geometryPart(metadata.base, buffer, start + metadata.assetBytes, metadata.baseBytes),
        stats: metadata.stats
      };
    }
    if (metadata.packetKind !== "archive" || !Array.isArray(metadata.members) || metadata.members.length > 2e4) {
      throw new Error("Invalid binary preview archive.");
    }
    let offset = 0;
    const names = /* @__PURE__ */ new Set();
    const members = metadata.members.map((row) => {
      if (!row || typeof row.name !== "string" || names.has(row.name) || !Number.isSafeInteger(row.size) || row.size < 0 || row.offset !== offset || row.size > size - offset) {
        throw new Error("Invalid binary preview member range.");
      }
      names.add(row.name);
      const data = bytes2.subarray(start + offset, start + offset + row.size);
      offset += row.size;
      return { name: row.name, size: row.size, data };
    });
    if (offset !== size) throw new Error("Trailing binary preview data.");
    const { packetKind, ...archive } = metadata;
    return { ...archive, members };
  }
  var HEADER_LIMIT, BODY_LIMIT, GEOMETRY_KEYS, UINT_KEYS;
  var init_packet = __esm({
    "worker/packet.mjs"() {
      "use strict";
      init_globals();
      HEADER_LIMIT = 4 * 1024 * 1024;
      BODY_LIMIT = 128 * 1024 * 1024;
      GEOMETRY_KEYS = [
        "positions",
        "uvs",
        "attachmentUvs",
        "tangentOffsets",
        "skinWeights",
        "detailUvs",
        "overlayUvs",
        "courtUvs",
        "graffitiUvs",
        "indices",
        "sourceVertexIds",
        "tangentAnchorIds",
        "skinOffsets",
        "skinBones"
      ];
      UINT_KEYS = /* @__PURE__ */ new Set(["indices", "sourceVertexIds", "tangentAnchorIds", "skinOffsets"]);
    }
  });

  // worker/native.js
  function resetNativeTransport() {
    transport = { binaryPackets: 0, binaryBytes: 0, jsonReplies: 0 };
  }
  function nativeTransportStats() {
    return { ...transport };
  }
  function nativeBuffer(data) {
    return typeof data === "string" ? import_buffer.Buffer.from(data, "base64") : import_buffer.Buffer.from(data.buffer, data.byteOffset, data.byteLength);
  }
  function native(command, payload) {
    return new Promise((resolve2, reject) => {
      const id = ++sequence;
      pending.set(id, { resolve: resolve2, reject });
      globalThis.postMessage({ kind: "native", id, command, payload });
    });
  }
  var sequence, pending, transport;
  var init_native = __esm({
    "worker/native.js"() {
      "use strict";
      init_globals();
      init_packet();
      sequence = 0;
      pending = /* @__PURE__ */ new Map();
      transport = { binaryPackets: 0, binaryBytes: 0, jsonReplies: 0 };
      globalThis.addEventListener("message", ({ data }) => {
        if (data.kind !== "native-reply") return;
        const task = pending.get(data.id);
        if (!task) return;
        pending.delete(data.id);
        if (data.error) task.reject(new Error(data.error));
        else {
          try {
            if (data.value instanceof ArrayBuffer) {
              const value = unpackPreviewPacket(data.value);
              transport.binaryPackets++;
              transport.binaryBytes += data.value.byteLength;
              task.resolve(value);
            } else {
              transport.jsonReplies++;
              task.resolve(data.value);
            }
          } catch (error) {
            task.reject(error);
          }
        }
      });
    }
  });

  // worker/fs.js
  var fs_exports = {};
  __export(fs_exports, {
    readFile: () => readFile,
    realpath: () => realpath,
    stat: () => stat
  });
  async function readFile(path) {
    return nativeBuffer((await native("read-file", { path })).data);
  }
  async function stat(path) {
    const value = await native("file-stat", { path });
    return { ...value, isFile: () => true };
  }
  async function realpath(path) {
    return (await stat(path)).path.replace(/^\\\\\?\\/, "");
  }
  var init_fs = __esm({
    "worker/fs.js"() {
      "use strict";
      init_globals();
      init_native();
    }
  });

  // worker/tangent-frame.js
  var tangent_frame_exports = {};
  __export(tangent_frame_exports, {
    frameCacheStats: () => frameCacheStats,
    projectTangentSpaceMeshes: () => projectTangentSpaceMeshes
  });
  function frameCacheStats() {
    return { entries: entries.size, bytes, budget: BUDGET, builds, hits };
  }
  function frameFor(baseMesh) {
    const key = baseMesh.geometryKey;
    const hit = key ? entries.get(key) : anonymous.get(baseMesh);
    if (hit) {
      hits++;
      if (key) {
        entries.delete(key);
        entries.set(key, hit);
      }
      return hit;
    }
    const faceVertices = /* @__PURE__ */ new Set();
    const normals = new Float32Array(baseMesh.positions.length);
    const tangents = new Float32Array(baseMesh.positions.length);
    for (const group of baseMesh.groups || []) {
      if (!/face|head|(?:^|_)ears?(?:_|$)/i.test(group.material)) continue;
      for (let cursor = group.start; cursor < group.start + group.count; cursor += 3) {
        const triangle = [baseMesh.indices[cursor], baseMesh.indices[cursor + 1], baseMesh.indices[cursor + 2]];
        triangle.forEach((vertex) => faceVertices.add(vertex));
        const [a, b, c] = triangle.map((vertex) => [
          baseMesh.positions[vertex * 3],
          baseMesh.positions[vertex * 3 + 1],
          baseMesh.positions[vertex * 3 + 2]
        ]);
        const ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        const ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        const normal = [
          ab[1] * ac[2] - ab[2] * ac[1],
          ab[2] * ac[0] - ab[0] * ac[2],
          ab[0] * ac[1] - ab[1] * ac[0]
        ];
        const [uvA, uvB, uvC] = triangle.map((vertex) => [
          baseMesh.uvs[vertex * 2],
          baseMesh.uvs[vertex * 2 + 1]
        ]);
        const duvAB = [uvB[0] - uvA[0], uvB[1] - uvA[1]];
        const duvAC = [uvC[0] - uvA[0], uvC[1] - uvA[1]];
        const divisor = duvAB[0] * duvAC[1] - duvAB[1] * duvAC[0];
        const tangent = Math.abs(divisor) > 1e-10 ? [
          (ab[0] * duvAC[1] - ac[0] * duvAB[1]) / divisor,
          (ab[1] * duvAC[1] - ac[1] * duvAB[1]) / divisor,
          (ab[2] * duvAC[1] - ac[2] * duvAB[1]) / divisor
        ] : ab;
        for (const vertex of triangle) {
          normals[vertex * 3] += normal[0];
          normals[vertex * 3 + 1] += normal[1];
          normals[vertex * 3 + 2] += normal[2];
          tangents[vertex * 3] += tangent[0];
          tangents[vertex * 3 + 1] += tangent[1];
          tangents[vertex * 3 + 2] += tangent[2];
        }
      }
    }
    if (!faceVertices.size) return null;
    for (const vertex of faceVertices) {
      const offset = vertex * 3;
      const length = Math.hypot(normals[offset], normals[offset + 1], normals[offset + 2]) || 1;
      normals[offset] /= length;
      normals[offset + 1] /= length;
      normals[offset + 2] /= length;
      const tangentLength = Math.hypot(tangents[offset], tangents[offset + 1], tangents[offset + 2]) || 1;
      tangents[offset] /= tangentLength;
      tangents[offset + 1] /= tangentLength;
      tangents[offset + 2] /= tangentLength;
    }
    const sourceVertexLookup = /* @__PURE__ */ new Map();
    baseMesh.sourceVertexIds?.forEach((sourceVertex, vertex) => sourceVertexLookup.set(sourceVertex, vertex));
    const value = { normals, tangents, sourceVertexLookup };
    const cost = normals.byteLength + tangents.byteLength + sourceVertexLookup.size * 32;
    builds++;
    if (key && cost <= BUDGET) {
      while (entries.size >= LIMIT || bytes + cost > BUDGET) {
        const oldest = entries.keys().next().value;
        const old = entries.get(oldest);
        bytes -= old.cost;
        entries.delete(oldest);
      }
      value.cost = cost;
      bytes += cost;
      entries.set(key, value);
    } else if (!key) anonymous.set(baseMesh, value);
    return value;
  }
  function projectTangentSpaceMeshes(preview2, basePreview) {
    const baseMesh = (basePreview?.meshes || []).find((mesh) => !mesh.attachmentUvs?.length && (mesh.groups || []).some((group) => /face|head|(?:^|_)ears?(?:_|$)/i.test(group.material)));
    if (!baseMesh?.positions?.length || !baseMesh?.uvs?.length) return 0;
    const frame = frameFor(baseMesh);
    if (!frame) return 0;
    const { normals, tangents, sourceVertexLookup } = frame;
    let projected = 0;
    for (const mesh of preview2?.meshes || []) {
      if (!mesh.tangentOffsets?.length || !mesh.tangentAnchorIds?.length) continue;
      const positions = new Float32Array(mesh.tangentOffsets.length);
      let matched = 0;
      for (let vertex = 0; vertex < mesh.tangentAnchorIds.length; vertex += 1) {
        const match = sourceVertexLookup.get(mesh.tangentAnchorIds[vertex]);
        if (match === void 0) continue;
        const offset = vertex * 3;
        const anchor = match * 3;
        const localX = mesh.tangentOffsets[offset] * 64;
        const localY = mesh.tangentOffsets[offset + 1] * 64;
        const localZ = mesh.tangentOffsets[offset + 2] * 64;
        const tangent = [tangents[anchor], tangents[anchor + 1], tangents[anchor + 2]];
        const normal = [normals[anchor], normals[anchor + 1], normals[anchor + 2]];
        const bitangent = [
          normal[1] * tangent[2] - normal[2] * tangent[1],
          normal[2] * tangent[0] - normal[0] * tangent[2],
          normal[0] * tangent[1] - normal[1] * tangent[0]
        ];
        const bitangentLength = Math.hypot(...bitangent) || 1;
        bitangent[0] /= bitangentLength;
        bitangent[1] /= bitangentLength;
        bitangent[2] /= bitangentLength;
        for (let axis = 0; axis < 3; axis += 1) {
          positions[offset + axis] = baseMesh.positions[anchor + axis] + tangent[axis] * localX + bitangent[axis] * localY + normal[axis] * localZ;
        }
        matched += 1;
      }
      if (!matched) continue;
      mesh.positions = positions;
      mesh.tangentProjected = true;
      mesh.tangentProjectionMode = "anchor-tangent-frame";
      projected += matched;
    }
    return projected;
  }
  var entries, anonymous, LIMIT, BUDGET, bytes, builds, hits;
  var init_tangent_frame = __esm({
    "worker/tangent-frame.js"() {
      "use strict";
      init_globals();
      entries = /* @__PURE__ */ new Map();
      anonymous = /* @__PURE__ */ new WeakMap();
      LIMIT = 8;
      BUDGET = 24 * 1024 * 1024;
      bytes = 0;
      builds = 0;
      hits = 0;
    }
  });

  // build-tools/node_modules/path-browserify/index.js
  var require_path_browserify = __commonJS({
    "build-tools/node_modules/path-browserify/index.js"(exports, module) {
      "use strict";
      init_globals();
      function assertPath(path) {
        if (typeof path !== "string") {
          throw new TypeError("Path must be a string. Received " + JSON.stringify(path));
        }
      }
      function normalizeStringPosix(path, allowAboveRoot) {
        var res = "";
        var lastSegmentLength = 0;
        var lastSlash = -1;
        var dots = 0;
        var code;
        for (var i = 0; i <= path.length; ++i) {
          if (i < path.length)
            code = path.charCodeAt(i);
          else if (code === 47)
            break;
          else
            code = 47;
          if (code === 47) {
            if (lastSlash === i - 1 || dots === 1) {
            } else if (lastSlash !== i - 1 && dots === 2) {
              if (res.length < 2 || lastSegmentLength !== 2 || res.charCodeAt(res.length - 1) !== 46 || res.charCodeAt(res.length - 2) !== 46) {
                if (res.length > 2) {
                  var lastSlashIndex = res.lastIndexOf("/");
                  if (lastSlashIndex !== res.length - 1) {
                    if (lastSlashIndex === -1) {
                      res = "";
                      lastSegmentLength = 0;
                    } else {
                      res = res.slice(0, lastSlashIndex);
                      lastSegmentLength = res.length - 1 - res.lastIndexOf("/");
                    }
                    lastSlash = i;
                    dots = 0;
                    continue;
                  }
                } else if (res.length === 2 || res.length === 1) {
                  res = "";
                  lastSegmentLength = 0;
                  lastSlash = i;
                  dots = 0;
                  continue;
                }
              }
              if (allowAboveRoot) {
                if (res.length > 0)
                  res += "/..";
                else
                  res = "..";
                lastSegmentLength = 2;
              }
            } else {
              if (res.length > 0)
                res += "/" + path.slice(lastSlash + 1, i);
              else
                res = path.slice(lastSlash + 1, i);
              lastSegmentLength = i - lastSlash - 1;
            }
            lastSlash = i;
            dots = 0;
          } else if (code === 46 && dots !== -1) {
            ++dots;
          } else {
            dots = -1;
          }
        }
        return res;
      }
      function _format(sep, pathObject) {
        var dir = pathObject.dir || pathObject.root;
        var base = pathObject.base || (pathObject.name || "") + (pathObject.ext || "");
        if (!dir) {
          return base;
        }
        if (dir === pathObject.root) {
          return dir + base;
        }
        return dir + sep + base;
      }
      var posix2 = {
        // path.resolve([from ...], to)
        resolve: function resolve2() {
          var resolvedPath = "";
          var resolvedAbsolute = false;
          var cwd;
          for (var i = arguments.length - 1; i >= -1 && !resolvedAbsolute; i--) {
            var path;
            if (i >= 0)
              path = arguments[i];
            else {
              if (cwd === void 0)
                cwd = process.cwd();
              path = cwd;
            }
            assertPath(path);
            if (path.length === 0) {
              continue;
            }
            resolvedPath = path + "/" + resolvedPath;
            resolvedAbsolute = path.charCodeAt(0) === 47;
          }
          resolvedPath = normalizeStringPosix(resolvedPath, !resolvedAbsolute);
          if (resolvedAbsolute) {
            if (resolvedPath.length > 0)
              return "/" + resolvedPath;
            else
              return "/";
          } else if (resolvedPath.length > 0) {
            return resolvedPath;
          } else {
            return ".";
          }
        },
        normalize: function normalize2(path) {
          assertPath(path);
          if (path.length === 0) return ".";
          var isAbsolute = path.charCodeAt(0) === 47;
          var trailingSeparator = path.charCodeAt(path.length - 1) === 47;
          path = normalizeStringPosix(path, !isAbsolute);
          if (path.length === 0 && !isAbsolute) path = ".";
          if (path.length > 0 && trailingSeparator) path += "/";
          if (isAbsolute) return "/" + path;
          return path;
        },
        isAbsolute: function isAbsolute(path) {
          assertPath(path);
          return path.length > 0 && path.charCodeAt(0) === 47;
        },
        join: function join2() {
          if (arguments.length === 0)
            return ".";
          var joined;
          for (var i = 0; i < arguments.length; ++i) {
            var arg = arguments[i];
            assertPath(arg);
            if (arg.length > 0) {
              if (joined === void 0)
                joined = arg;
              else
                joined += "/" + arg;
            }
          }
          if (joined === void 0)
            return ".";
          return posix2.normalize(joined);
        },
        relative: function relative(from, to) {
          assertPath(from);
          assertPath(to);
          if (from === to) return "";
          from = posix2.resolve(from);
          to = posix2.resolve(to);
          if (from === to) return "";
          var fromStart = 1;
          for (; fromStart < from.length; ++fromStart) {
            if (from.charCodeAt(fromStart) !== 47)
              break;
          }
          var fromEnd = from.length;
          var fromLen = fromEnd - fromStart;
          var toStart = 1;
          for (; toStart < to.length; ++toStart) {
            if (to.charCodeAt(toStart) !== 47)
              break;
          }
          var toEnd = to.length;
          var toLen = toEnd - toStart;
          var length = fromLen < toLen ? fromLen : toLen;
          var lastCommonSep = -1;
          var i = 0;
          for (; i <= length; ++i) {
            if (i === length) {
              if (toLen > length) {
                if (to.charCodeAt(toStart + i) === 47) {
                  return to.slice(toStart + i + 1);
                } else if (i === 0) {
                  return to.slice(toStart + i);
                }
              } else if (fromLen > length) {
                if (from.charCodeAt(fromStart + i) === 47) {
                  lastCommonSep = i;
                } else if (i === 0) {
                  lastCommonSep = 0;
                }
              }
              break;
            }
            var fromCode = from.charCodeAt(fromStart + i);
            var toCode = to.charCodeAt(toStart + i);
            if (fromCode !== toCode)
              break;
            else if (fromCode === 47)
              lastCommonSep = i;
          }
          var out = "";
          for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
            if (i === fromEnd || from.charCodeAt(i) === 47) {
              if (out.length === 0)
                out += "..";
              else
                out += "/..";
            }
          }
          if (out.length > 0)
            return out + to.slice(toStart + lastCommonSep);
          else {
            toStart += lastCommonSep;
            if (to.charCodeAt(toStart) === 47)
              ++toStart;
            return to.slice(toStart);
          }
        },
        _makeLong: function _makeLong(path) {
          return path;
        },
        dirname: function dirname2(path) {
          assertPath(path);
          if (path.length === 0) return ".";
          var code = path.charCodeAt(0);
          var hasRoot = code === 47;
          var end = -1;
          var matchedSlash = true;
          for (var i = path.length - 1; i >= 1; --i) {
            code = path.charCodeAt(i);
            if (code === 47) {
              if (!matchedSlash) {
                end = i;
                break;
              }
            } else {
              matchedSlash = false;
            }
          }
          if (end === -1) return hasRoot ? "/" : ".";
          if (hasRoot && end === 1) return "//";
          return path.slice(0, end);
        },
        basename: function basename2(path, ext) {
          if (ext !== void 0 && typeof ext !== "string") throw new TypeError('"ext" argument must be a string');
          assertPath(path);
          var start = 0;
          var end = -1;
          var matchedSlash = true;
          var i;
          if (ext !== void 0 && ext.length > 0 && ext.length <= path.length) {
            if (ext.length === path.length && ext === path) return "";
            var extIdx = ext.length - 1;
            var firstNonSlashEnd = -1;
            for (i = path.length - 1; i >= 0; --i) {
              var code = path.charCodeAt(i);
              if (code === 47) {
                if (!matchedSlash) {
                  start = i + 1;
                  break;
                }
              } else {
                if (firstNonSlashEnd === -1) {
                  matchedSlash = false;
                  firstNonSlashEnd = i + 1;
                }
                if (extIdx >= 0) {
                  if (code === ext.charCodeAt(extIdx)) {
                    if (--extIdx === -1) {
                      end = i;
                    }
                  } else {
                    extIdx = -1;
                    end = firstNonSlashEnd;
                  }
                }
              }
            }
            if (start === end) end = firstNonSlashEnd;
            else if (end === -1) end = path.length;
            return path.slice(start, end);
          } else {
            for (i = path.length - 1; i >= 0; --i) {
              if (path.charCodeAt(i) === 47) {
                if (!matchedSlash) {
                  start = i + 1;
                  break;
                }
              } else if (end === -1) {
                matchedSlash = false;
                end = i + 1;
              }
            }
            if (end === -1) return "";
            return path.slice(start, end);
          }
        },
        extname: function extname2(path) {
          assertPath(path);
          var startDot = -1;
          var startPart = 0;
          var end = -1;
          var matchedSlash = true;
          var preDotState = 0;
          for (var i = path.length - 1; i >= 0; --i) {
            var code = path.charCodeAt(i);
            if (code === 47) {
              if (!matchedSlash) {
                startPart = i + 1;
                break;
              }
              continue;
            }
            if (end === -1) {
              matchedSlash = false;
              end = i + 1;
            }
            if (code === 46) {
              if (startDot === -1)
                startDot = i;
              else if (preDotState !== 1)
                preDotState = 1;
            } else if (startDot !== -1) {
              preDotState = -1;
            }
          }
          if (startDot === -1 || end === -1 || // We saw a non-dot character immediately before the dot
          preDotState === 0 || // The (right-most) trimmed path component is exactly '..'
          preDotState === 1 && startDot === end - 1 && startDot === startPart + 1) {
            return "";
          }
          return path.slice(startDot, end);
        },
        format: function format(pathObject) {
          if (pathObject === null || typeof pathObject !== "object") {
            throw new TypeError('The "pathObject" argument must be of type Object. Received type ' + typeof pathObject);
          }
          return _format("/", pathObject);
        },
        parse: function parse2(path) {
          assertPath(path);
          var ret = { root: "", dir: "", base: "", ext: "", name: "" };
          if (path.length === 0) return ret;
          var code = path.charCodeAt(0);
          var isAbsolute = code === 47;
          var start;
          if (isAbsolute) {
            ret.root = "/";
            start = 1;
          } else {
            start = 0;
          }
          var startDot = -1;
          var startPart = 0;
          var end = -1;
          var matchedSlash = true;
          var i = path.length - 1;
          var preDotState = 0;
          for (; i >= start; --i) {
            code = path.charCodeAt(i);
            if (code === 47) {
              if (!matchedSlash) {
                startPart = i + 1;
                break;
              }
              continue;
            }
            if (end === -1) {
              matchedSlash = false;
              end = i + 1;
            }
            if (code === 46) {
              if (startDot === -1) startDot = i;
              else if (preDotState !== 1) preDotState = 1;
            } else if (startDot !== -1) {
              preDotState = -1;
            }
          }
          if (startDot === -1 || end === -1 || // We saw a non-dot character immediately before the dot
          preDotState === 0 || // The (right-most) trimmed path component is exactly '..'
          preDotState === 1 && startDot === end - 1 && startDot === startPart + 1) {
            if (end !== -1) {
              if (startPart === 0 && isAbsolute) ret.base = ret.name = path.slice(1, end);
              else ret.base = ret.name = path.slice(startPart, end);
            }
          } else {
            if (startPart === 0 && isAbsolute) {
              ret.name = path.slice(1, startDot);
              ret.base = path.slice(1, end);
            } else {
              ret.name = path.slice(startPart, startDot);
              ret.base = path.slice(startPart, end);
            }
            ret.ext = path.slice(startDot, end);
          }
          if (startPart > 0) ret.dir = path.slice(0, startPart - 1);
          else if (isAbsolute) ret.dir = "/";
          return ret;
        },
        sep: "/",
        delimiter: ":",
        win32: null,
        posix: null
      };
      posix2.posix = posix2;
      module.exports = posix2;
    }
  });

  // worker/path.js
  var path_exports = {};
  __export(path_exports, {
    basename: () => basename,
    dirname: () => dirname,
    extname: () => extname,
    join: () => join,
    parse: () => parse,
    posix: () => import_path_browserify.default,
    resolve: () => resolve
  });
  var import_path_browserify, normalize, basename, dirname, extname, parse, resolve, join;
  var init_path = __esm({
    "worker/path.js"() {
      "use strict";
      init_globals();
      import_path_browserify = __toESM(require_path_browserify());
      normalize = (value) => String(value).replaceAll("\\", "/");
      basename = (value, suffix) => import_path_browserify.default.basename(normalize(value), suffix);
      dirname = (value) => import_path_browserify.default.dirname(normalize(value));
      extname = (value) => import_path_browserify.default.extname(normalize(value));
      parse = (value) => import_path_browserify.default.parse(normalize(value));
      resolve = (value) => normalize(value);
      join = (...values) => import_path_browserify.default.join(...values.map(normalize));
    }
  });

  // build-tools/node_modules/fflate/esm/browser.js
  function inflateSync(data, opts) {
    return inflt(data, { i: 2 }, opts && opts.out, opts && opts.dictionary);
  }
  var u8, u16, i32, fleb, fdeb, clim, freb, _a, fl, revfl, _b, fd, revfd, rev, x, i, hMap, flt, i, i, i, i, fdt, i, flrm, fdrm, max, bits, bits16, shft, slc, ec, err, inflt, et, td, tds;
  var init_browser = __esm({
    "build-tools/node_modules/fflate/esm/browser.js"() {
      init_globals();
      u8 = Uint8Array;
      u16 = Uint16Array;
      i32 = Int32Array;
      fleb = new u8([
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        1,
        1,
        1,
        1,
        2,
        2,
        2,
        2,
        3,
        3,
        3,
        3,
        4,
        4,
        4,
        4,
        5,
        5,
        5,
        5,
        0,
        /* unused */
        0,
        0,
        /* impossible */
        0
      ]);
      fdeb = new u8([
        0,
        0,
        0,
        0,
        1,
        1,
        2,
        2,
        3,
        3,
        4,
        4,
        5,
        5,
        6,
        6,
        7,
        7,
        8,
        8,
        9,
        9,
        10,
        10,
        11,
        11,
        12,
        12,
        13,
        13,
        /* unused */
        0,
        0
      ]);
      clim = new u8([16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15]);
      freb = function(eb, start) {
        var b = new u16(31);
        for (var i = 0; i < 31; ++i) {
          b[i] = start += 1 << eb[i - 1];
        }
        var r = new i32(b[30]);
        for (var i = 1; i < 30; ++i) {
          for (var j = b[i]; j < b[i + 1]; ++j) {
            r[j] = j - b[i] << 5 | i;
          }
        }
        return { b, r };
      };
      _a = freb(fleb, 2);
      fl = _a.b;
      revfl = _a.r;
      fl[28] = 258, revfl[258] = 28;
      _b = freb(fdeb, 0);
      fd = _b.b;
      revfd = _b.r;
      rev = new u16(32768);
      for (i = 0; i < 32768; ++i) {
        x = (i & 43690) >> 1 | (i & 21845) << 1;
        x = (x & 52428) >> 2 | (x & 13107) << 2;
        x = (x & 61680) >> 4 | (x & 3855) << 4;
        rev[i] = ((x & 65280) >> 8 | (x & 255) << 8) >> 1;
      }
      hMap = (function(cd, mb, r) {
        var s = cd.length;
        var i = 0;
        var l = new u16(mb);
        for (; i < s; ++i) {
          if (cd[i])
            ++l[cd[i] - 1];
        }
        var le = new u16(mb);
        for (i = 1; i < mb; ++i) {
          le[i] = le[i - 1] + l[i - 1] << 1;
        }
        var co;
        if (r) {
          co = new u16(1 << mb);
          var rvb = 15 - mb;
          for (i = 0; i < s; ++i) {
            if (cd[i]) {
              var sv = i << 4 | cd[i];
              var r_1 = mb - cd[i];
              var v = le[cd[i] - 1]++ << r_1;
              for (var m = v | (1 << r_1) - 1; v <= m; ++v) {
                co[rev[v] >> rvb] = sv;
              }
            }
          }
        } else {
          co = new u16(s);
          for (i = 0; i < s; ++i) {
            if (cd[i]) {
              co[i] = rev[le[cd[i] - 1]++] >> 15 - cd[i];
            }
          }
        }
        return co;
      });
      flt = new u8(288);
      for (i = 0; i < 144; ++i)
        flt[i] = 8;
      for (i = 144; i < 256; ++i)
        flt[i] = 9;
      for (i = 256; i < 280; ++i)
        flt[i] = 7;
      for (i = 280; i < 288; ++i)
        flt[i] = 8;
      fdt = new u8(32);
      for (i = 0; i < 32; ++i)
        fdt[i] = 5;
      flrm = /* @__PURE__ */ hMap(flt, 9, 1);
      fdrm = /* @__PURE__ */ hMap(fdt, 5, 1);
      max = function(a) {
        var m = a[0];
        for (var i = 1; i < a.length; ++i) {
          if (a[i] > m)
            m = a[i];
        }
        return m;
      };
      bits = function(d, p, m) {
        var o = p / 8 | 0;
        return (d[o] | d[o + 1] << 8) >> (p & 7) & m;
      };
      bits16 = function(d, p) {
        var o = p / 8 | 0;
        return (d[o] | d[o + 1] << 8 | d[o + 2] << 16) >> (p & 7);
      };
      shft = function(p) {
        return (p + 7) / 8 | 0;
      };
      slc = function(v, s, e) {
        if (s == null || s < 0)
          s = 0;
        if (e == null || e > v.length)
          e = v.length;
        return new u8(v.subarray(s, e));
      };
      ec = [
        "unexpected EOF",
        "invalid block type",
        "invalid length/literal",
        "invalid distance",
        "stream finished",
        "no stream handler",
        ,
        // determined by compression function
        "no callback",
        "invalid UTF-8 data",
        "extra field too long",
        "date not in range 1980-2099",
        "filename too long",
        "stream finishing",
        "invalid zip data"
        // determined by unknown compression method
      ];
      err = function(ind, msg, nt) {
        var e = new Error(msg || ec[ind]);
        e.code = ind;
        if (Error.captureStackTrace)
          Error.captureStackTrace(e, err);
        if (!nt)
          throw e;
        return e;
      };
      inflt = function(dat, st, buf, dict) {
        var sl = dat.length, dl = dict ? dict.length : 0;
        if (!sl || st.f && !st.l)
          return buf || new u8(0);
        var noBuf = !buf;
        var resize = noBuf || st.i != 2;
        var noSt = st.i;
        if (noBuf)
          buf = new u8(sl * 3);
        var cbuf = function(l2) {
          var bl = buf.length;
          if (l2 > bl) {
            var nbuf = new u8(Math.max(bl * 2, l2));
            nbuf.set(buf);
            buf = nbuf;
          }
        };
        var final = st.f || 0, pos = st.p || 0, bt = st.b || 0, lm = st.l, dm = st.d, lbt = st.m, dbt = st.n;
        var tbts = sl * 8;
        do {
          if (!lm) {
            final = bits(dat, pos, 1);
            var type = bits(dat, pos + 1, 3);
            pos += 3;
            if (!type) {
              var s = shft(pos) + 4, l = dat[s - 4] | dat[s - 3] << 8, t = s + l;
              if (t > sl) {
                if (noSt)
                  err(0);
                break;
              }
              if (resize)
                cbuf(bt + l);
              buf.set(dat.subarray(s, t), bt);
              st.b = bt += l, st.p = pos = t * 8, st.f = final;
              continue;
            } else if (type == 1)
              lm = flrm, dm = fdrm, lbt = 9, dbt = 5;
            else if (type == 2) {
              var hLit = bits(dat, pos, 31) + 257, hcLen = bits(dat, pos + 10, 15) + 4;
              var tl = hLit + bits(dat, pos + 5, 31) + 1;
              pos += 14;
              var ldt = new u8(tl);
              var clt = new u8(19);
              for (var i = 0; i < hcLen; ++i) {
                clt[clim[i]] = bits(dat, pos + i * 3, 7);
              }
              pos += hcLen * 3;
              var clb = max(clt), clbmsk = (1 << clb) - 1;
              var clm = hMap(clt, clb, 1);
              for (var i = 0; i < tl; ) {
                var r = clm[bits(dat, pos, clbmsk)];
                pos += r & 15;
                var s = r >> 4;
                if (s < 16) {
                  ldt[i++] = s;
                } else {
                  var c = 0, n = 0;
                  if (s == 16)
                    n = 3 + bits(dat, pos, 3), pos += 2, c = ldt[i - 1];
                  else if (s == 17)
                    n = 3 + bits(dat, pos, 7), pos += 3;
                  else if (s == 18)
                    n = 11 + bits(dat, pos, 127), pos += 7;
                  while (n--)
                    ldt[i++] = c;
                }
              }
              var lt = ldt.subarray(0, hLit), dt = ldt.subarray(hLit);
              lbt = max(lt);
              dbt = max(dt);
              lm = hMap(lt, lbt, 1);
              dm = hMap(dt, dbt, 1);
            } else
              err(1);
            if (pos > tbts) {
              if (noSt)
                err(0);
              break;
            }
          }
          if (resize)
            cbuf(bt + 131072);
          var lms = (1 << lbt) - 1, dms = (1 << dbt) - 1;
          var lpos = pos;
          for (; ; lpos = pos) {
            var c = lm[bits16(dat, pos) & lms], sym = c >> 4;
            pos += c & 15;
            if (pos > tbts) {
              if (noSt)
                err(0);
              break;
            }
            if (!c)
              err(2);
            if (sym < 256)
              buf[bt++] = sym;
            else if (sym == 256) {
              lpos = pos, lm = null;
              break;
            } else {
              var add = sym - 254;
              if (sym > 264) {
                var i = sym - 257, b = fleb[i];
                add = bits(dat, pos, (1 << b) - 1) + fl[i];
                pos += b;
              }
              var d = dm[bits16(dat, pos) & dms], dsym = d >> 4;
              if (!d)
                err(3);
              pos += d & 15;
              var dt = fd[dsym];
              if (dsym > 3) {
                var b = fdeb[dsym];
                dt += bits16(dat, pos) & (1 << b) - 1, pos += b;
              }
              if (pos > tbts) {
                if (noSt)
                  err(0);
                break;
              }
              if (resize)
                cbuf(bt + 131072);
              var end = bt + add;
              if (bt < dt) {
                var shift = dl - dt, dend = Math.min(dt, end);
                if (shift + bt < 0)
                  err(3);
                for (; bt < dend; ++bt)
                  buf[bt] = dict[shift + bt];
              }
              for (; bt < end; ++bt)
                buf[bt] = buf[bt - dt];
            }
          }
          st.l = lm, st.p = lpos, st.b = bt, st.f = final;
          if (lm)
            final = 1, st.m = lbt, st.d = dm, st.n = dbt;
        } while (!final);
        return bt != buf.length && noBuf ? slc(buf, 0, bt) : buf.subarray(0, bt);
      };
      et = /* @__PURE__ */ new u8(0);
      td = typeof TextDecoder != "undefined" && /* @__PURE__ */ new TextDecoder();
      tds = 0;
      try {
        td.decode(et, { stream: true });
        tds = 1;
      } catch (e) {
      }
    }
  });

  // worker/zlib.js
  var zlib_exports = {};
  __export(zlib_exports, {
    inflateRawSync: () => inflateRawSync
  });
  function inflateRawSync(input, options = {}) {
    const bytes2 = inflateSync(input);
    if (bytes2.length > (options.maxOutputLength || 512 * 1024 * 1024)) throw new Error("Preview resource exceeds its decoded size limit.");
    return import_buffer.Buffer.from(bytes2);
  }
  var init_zlib = __esm({
    "worker/zlib.js"() {
      "use strict";
      init_globals();
      init_browser();
    }
  });

  // ../character-mod-tool-webview2/Reference/src/core/iff.js
  var require_iff = __commonJS({
    "../character-mod-tool-webview2/Reference/src/core/iff.js"(exports, module) {
      init_globals();
      var fsp = (init_fs(), __toCommonJS(fs_exports));
      var path = (init_path(), __toCommonJS(path_exports));
      var zlib = (init_zlib(), __toCommonJS(zlib_exports));
      var EOCD_SIGNATURE = 101010256;
      var CENTRAL_SIGNATURE = 33639248;
      var LOCAL_SIGNATURE = 67324752;
      var MAX_EOCD_SEARCH = 65535 + 22;
      var crcTable;
      function getCrcTable() {
        if (crcTable) return crcTable;
        crcTable = new Uint32Array(256);
        for (let value = 0; value < 256; value += 1) {
          let crc = value;
          for (let bit = 0; bit < 8; bit += 1) crc = crc >>> 1 ^ (crc & 1 ? 3988292384 : 0);
          crcTable[value] = crc >>> 0;
        }
        return crcTable;
      }
      function crc32(buffer) {
        const table = getCrcTable();
        let crc = 4294967295;
        for (const byte of buffer) crc = crc >>> 8 ^ table[(crc ^ byte) & 255];
        return (crc ^ 4294967295) >>> 0;
      }
      function findEocd(buffer) {
        const lowerBound = Math.max(0, buffer.length - MAX_EOCD_SEARCH);
        for (let offset = buffer.length - 22; offset >= lowerBound; offset -= 1) {
          if (buffer.readUInt32LE(offset) === EOCD_SIGNATURE) return offset;
        }
        throw new Error("IFF does not contain a valid ZIP end record.");
      }
      function parseZip(buffer) {
        const eocd = findEocd(buffer);
        const disk = buffer.readUInt16LE(eocd + 4);
        const centralDisk = buffer.readUInt16LE(eocd + 6);
        const diskEntries = buffer.readUInt16LE(eocd + 8);
        const totalEntries = buffer.readUInt16LE(eocd + 10);
        const centralSize = buffer.readUInt32LE(eocd + 12);
        const centralOffset = buffer.readUInt32LE(eocd + 16);
        if (disk !== 0 || centralDisk !== 0 || diskEntries !== totalEntries) {
          throw new Error("Multi-disk IFF archives are not supported.");
        }
        if (totalEntries === 65535 || centralOffset === 4294967295 || centralSize === 4294967295) {
          throw new Error("ZIP64 IFF archives are not supported.");
        }
        if (centralOffset + centralSize > eocd || centralOffset < 0) {
          throw new Error("IFF central directory is outside the archive.");
        }
        const entries2 = [];
        let cursor = centralOffset;
        for (let index = 0; index < totalEntries; index += 1) {
          if (cursor + 46 > buffer.length || buffer.readUInt32LE(cursor) !== CENTRAL_SIGNATURE) {
            throw new Error(`Invalid IFF central-directory record ${index + 1}.`);
          }
          const flags = buffer.readUInt16LE(cursor + 8);
          const method = buffer.readUInt16LE(cursor + 10);
          const expectedCrc = buffer.readUInt32LE(cursor + 16);
          const compressedSize = buffer.readUInt32LE(cursor + 20);
          const size = buffer.readUInt32LE(cursor + 24);
          const nameLength = buffer.readUInt16LE(cursor + 28);
          const extraLength = buffer.readUInt16LE(cursor + 30);
          const commentLength = buffer.readUInt16LE(cursor + 32);
          const localOffset = buffer.readUInt32LE(cursor + 42);
          const next = cursor + 46 + nameLength + extraLength + commentLength;
          if (next > buffer.length) throw new Error("Truncated IFF central directory.");
          if (flags & 1) throw new Error("Encrypted IFF members are not supported.");
          if (![0, 8].includes(method)) throw new Error(`Unsupported ZIP compression method ${method}.`);
          const name = buffer.subarray(cursor + 46, cursor + 46 + nameLength).toString("utf8").replaceAll("\\", "/");
          if (!name || name.includes("\0") || name.startsWith("/") || name.split("/").some((part) => part === "..")) {
            throw new Error(`Unsafe IFF member name: ${name || "(empty)"}.`);
          }
          if (localOffset + 30 > buffer.length || buffer.readUInt32LE(localOffset) !== LOCAL_SIGNATURE) {
            throw new Error(`IFF member ${name} has an invalid local header.`);
          }
          const localNameLength = buffer.readUInt16LE(localOffset + 26);
          const localExtraLength = buffer.readUInt16LE(localOffset + 28);
          const dataOffset = localOffset + 30 + localNameLength + localExtraLength;
          if (dataOffset + compressedSize > buffer.length) throw new Error(`IFF member ${name} is truncated.`);
          entries2.push({ name, flags, method, expectedCrc, compressedSize, size, dataOffset });
          cursor = next;
        }
        return { entries: entries2, centralOffset, centralSize };
      }
      function readZipEntry(buffer, entry, verify = true) {
        const packed = buffer.subarray(entry.dataOffset, entry.dataOffset + entry.compressedSize);
        const decoded = entry.method === 0 ? import_buffer.Buffer.from(packed) : zlib.inflateRawSync(packed, { maxOutputLength: entry.size });
        if (decoded.length !== entry.size) throw new Error(`IFF member ${entry.name} decoded to the wrong size.`);
        if (verify && crc32(decoded) !== entry.expectedCrc) throw new Error(`IFF member ${entry.name} failed CRC validation.`);
        return decoded;
      }
      function entryLookup(entries2) {
        const lookup = /* @__PURE__ */ new Map();
        for (const entry of entries2) {
          lookup.set(entry.name.toLowerCase(), entry);
          const leaf = path.posix.basename(entry.name).toLowerCase();
          if (!lookup.has(leaf)) lookup.set(leaf, entry);
        }
        return lookup;
      }
      function parseScneText(buffer) {
        const text = buffer.toString("utf8").replace(/^\uFEFF/, "").trim();
        if (!text) return {};
        return JSON.parse(text.startsWith("{") ? text : `{${text}}`);
      }
      function parseBinaryTree(buffer) {
        const RECORD_START = 24;
        const RECORD_SIZE = 40;
        if (!import_buffer.Buffer.isBuffer(buffer) || buffer.length < RECORD_START || buffer.readUInt32LE(0) !== 3760191633 || buffer.readUInt32LE(4) !== 1) return null;
        const stringBytes = buffer.readUInt32LE(8);
        const nodeCount = buffer.readUInt32LE(12);
        const stringStart = buffer.length - stringBytes;
        if (!stringBytes || !nodeCount || stringStart < RECORD_START + nodeCount * RECORD_SIZE) return null;
        function relativeString(pointerOffset) {
          const relative = Number(buffer.readBigInt64LE(pointerOffset));
          const start = pointerOffset + relative - 1;
          if (start < stringStart || start >= buffer.length) throw new Error("Binary-tree string pointer is out of bounds.");
          const end = buffer.indexOf(0, start);
          if (end < start) throw new Error("Binary-tree string is unterminated.");
          return buffer.toString("utf8", start, end);
        }
        function recordOffset(index) {
          if (index < 0 || index >= nodeCount) throw new Error("Binary-tree node index is out of bounds.");
          return RECORD_START + index * RECORD_SIZE;
        }
        function nodeName(index) {
          return relativeString(index === 0 ? 16 : recordOffset(index - 1) + 32);
        }
        let cursor = 0;
        function readNode(unnamed = false) {
          const index = cursor;
          cursor += 1;
          const offset = recordOffset(index);
          const fullTag = buffer.readUInt32LE(offset + 4);
          if ((fullTag & 4294967040) !== 1073741824) throw new Error("Unsupported binary-tree value tag.");
          const tag = fullTag & 255;
          const name = unnamed ? "" : nodeName(index);
          let value;
          if (tag === 0) value = null;
          else if (tag === 3) value = relativeString(offset + 16);
          else if (tag === 5) value = buffer.readDoubleLE(offset + 16);
          else if (tag === 6) {
            const integer = buffer.readBigUInt64LE(offset + 16);
            value = integer <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(integer) : integer.toString();
          } else if (tag === 1 || tag === 2) {
            const count = buffer.readUInt32LE(offset + 12);
            if (tag === 2) {
              value = [];
              for (let child = 0; child < count; child += 1) value.push(readNode(true).value);
            } else {
              value = {};
              for (let child = 0; child < count; child += 1) {
                const parsed = readNode(false);
                value[parsed.name] = parsed.value;
              }
            }
          } else throw new Error(`Unsupported binary-tree value tag ${tag}.`);
          return { name, value };
        }
        const root = readNode(false);
        if (cursor !== nodeCount) throw new Error("Binary-tree node count is inconsistent.");
        return { [root.name]: root.value };
      }
      function scneBinaryReferences(text) {
        const references = [];
        const pattern = /"Binary"\s*:\s*"([^"]+)"/g;
        for (let match = pattern.exec(text); match; match = pattern.exec(text)) references.push(match[1].replaceAll("\\", "/"));
        return references;
      }
      async function loadIff(filePath) {
        const buffer = await fsp.readFile(filePath);
        const zip = parseZip(buffer);
        return { buffer, ...zip, lookup: entryLookup(zip.entries) };
      }
      module.exports = {
        crc32,
        entryLookup,
        loadIff,
        parseBinaryTree,
        parseScneText,
        parseZip,
        readZipEntry,
        scneBinaryReferences
      };
    }
  });

  // worker/iff.js
  var require_iff2 = __commonJS({
    "worker/iff.js"(exports, module) {
      init_globals();
      init_native();
      var original = require_iff();
      async function loadIff(filePath) {
        if (!String(filePath).startsWith("cmt-preview://")) return original.loadIff(filePath);
        try {
          const archive = await native("read-archive", { path: filePath });
          const entries2 = archive.members.map((member) => ({
            name: member.name,
            size: member.size,
            decoded: nativeBuffer(member.data)
          }));
          return {
            buffer: null,
            entries: entries2,
            lookup: original.entryLookup(entries2),
            geometryKey: archive.geometryKey,
            contentKey: archive.contentKey,
            warnings: archive.warnings || []
          };
        } catch (error) {
          if (String(filePath).startsWith("cmt-preview://")) throw error;
          return original.loadIff(filePath);
        }
      }
      function readZipEntry(buffer, entry, verify = true) {
        return entry.decoded || original.readZipEntry(buffer, entry, verify);
      }
      module.exports = { ...original, loadIff, readZipEntry };
    }
  });

  // ../character-mod-tool-webview2/Reference/src/core/preview.js
  var require_preview = __commonJS({
    "../character-mod-tool-webview2/Reference/src/core/preview.js"(exports, module) {
      init_globals();
      var path = (init_path(), __toCommonJS(path_exports));
      var fsp = (init_fs(), __toCommonJS(fs_exports));
      var { loadIff, parseBinaryTree, parseScneText, readZipEntry } = require_iff2();
      var DEFAULT_MAX_TRIANGLES = 25e4;
      var DEFAULT_MAX_MODELS = 300;
      var MAX_TEXTURE_BYTES = 32 * 1024 * 1024;
      var MAX_ARENA_TEXTURE_REQUESTS = 96;
      function sceneNameVariants(value) {
        const text = String(value || "");
        const values = /* @__PURE__ */ new Set([text, path.posix.basename(text.replaceAll("\\", "/"))]);
        if (text.includes(":")) values.add(text.slice(text.lastIndexOf(":") + 1));
        if (text.includes("@GameObjects@")) {
          const rest = text.split("@GameObjects@", 2)[1];
          values.add(rest);
          if (rest.includes(":")) {
            values.add(rest.split(":", 1)[0]);
            values.add(rest.slice(rest.lastIndexOf(":") + 1));
          }
        }
        return [...values].map((item) => String(item || "").trim().toLowerCase()).filter(Boolean);
      }
      function findSceneRoots(value, output = []) {
        if (!value || typeof value !== "object") return output;
        if (!Array.isArray(value) && value.Object && value.Model && typeof value.Object === "object" && typeof value.Model === "object") output.push(value);
        for (const child of Object.values(value)) findSceneRoots(child, output);
        return output;
      }
      function addPlacement(lookup, variant, placement) {
        const bucket = lookup.get(variant) || [];
        if (!bucket.includes(placement)) bucket.push(placement);
        lookup.set(variant, bucket);
      }
      function cumulativeTransformTranslation(transformMap, transformName) {
        const entries2 = Object.entries(transformMap || {});
        let index = entries2.findIndex(([name]) => name === transformName);
        const translation = [0, 0, 0];
        const visited = /* @__PURE__ */ new Set();
        while (index >= 0 && index < entries2.length && !visited.has(index)) {
          visited.add(index);
          const node = entries2[index][1];
          if (!node || typeof node !== "object") break;
          const value = Array.isArray(node.Translate) ? node.Translate : [];
          for (let axis = 0; axis < 3; axis += 1) translation[axis] += Number(value[axis] || 0);
          index = Number.isInteger(node.Parent) ? node.Parent : -1;
        }
        return translation;
      }
      function findTransformOffset(root, objectValue) {
        const objectTransform = String(objectValue?.Transform || "");
        if (!objectTransform.includes("|")) return [0, 0, 0];
        const pivotName = objectTransform.slice(objectTransform.lastIndexOf("|") + 1).toLowerCase();
        const assembly = objectTransform.split(":", 1)[0].toLowerCase();
        const candidates = [];
        for (const model of Object.values(root.Model || {})) {
          const transformMap = model?.Transform;
          if (!transformMap || typeof transformMap !== "object") continue;
          for (const transformName of Object.keys(transformMap)) {
            if (transformName.slice(transformName.lastIndexOf("|") + 1).toLowerCase() !== pivotName) continue;
            const sameAssembly = transformName.split(":", 1)[0].toLowerCase() === assembly;
            const depth = transformName.split("|").length;
            const translation = cumulativeTransformTranslation(transformMap, transformName);
            const magnitude = translation.reduce((sum, value) => sum + Math.abs(value), 0);
            candidates.push({ sameAssembly, depth, magnitude, translation });
          }
        }
        candidates.sort((a, b) => Number(b.sameAssembly) - Number(a.sameAssembly) || b.depth - a.depth || b.magnitude - a.magnitude);
        return candidates[0]?.translation || [0, 0, 0];
      }
      function applyLocalTranslation(matrix, translation) {
        const [x, y, z] = translation;
        if (!x && !y && !z) return [...matrix];
        const result = [...matrix];
        result[12] = matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12];
        result[13] = matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13];
        result[14] = matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14];
        return result;
      }
      function buildSceneTransformLookup(scene) {
        const lookup = /* @__PURE__ */ new Map();
        let objectMatrices = 0;
        for (const root of findSceneRoots(scene)) {
          for (const [objectKey, objectValue] of Object.entries(root.Object || {})) {
            const target = objectValue?.Target;
            const matrix = objectValue?.Matrix;
            if (!target || !Array.isArray(matrix) || matrix.length < 16) continue;
            const numericMatrix = matrix.slice(0, 16).map(Number);
            if (numericMatrix.some((value) => !Number.isFinite(value))) continue;
            const transformOffset = findTransformOffset(root, objectValue);
            const placement = {
              name: objectKey,
              target,
              matrix: applyLocalTranslation(numericMatrix, transformOffset),
              transform: String(objectValue.Transform || ""),
              transformOffset
            };
            const variants = /* @__PURE__ */ new Set([...sceneNameVariants(target), ...sceneNameVariants(objectKey)]);
            const model = root.Model?.[target];
            if (model && typeof model === "object") {
              for (const primitive of model.Prim || []) {
                for (const key of ["Mesh", "Material"]) {
                  for (const variant of sceneNameVariants(primitive?.[key])) variants.add(variant);
                }
              }
              for (const key of ["IndexBuffer", "VertexStream"]) {
                const values = Array.isArray(model[key]) ? model[key] : Object.values(model[key] || {});
                for (const value of values) {
                  for (const variant of sceneNameVariants(value?.Binary)) variants.add(variant);
                }
              }
            }
            for (const variant of variants) addPlacement(lookup, variant, placement);
            objectMatrices += 1;
          }
        }
        return { lookup, objectMatrices };
      }
      function lookupScenePlacements(modelPath, model, lookup) {
        const candidates = new Set(sceneNameVariants(modelPath));
        for (const primitive of model.Prim || []) {
          for (const key of ["Mesh", "Material"]) {
            for (const variant of sceneNameVariants(primitive?.[key])) candidates.add(variant);
          }
        }
        for (const candidate of candidates) {
          const placements = lookup.get(candidate);
          if (placements?.length) return placements;
        }
        return [];
      }
      function gameMatrixToPreviewMatrix(matrix) {
        const m = matrix.slice(0, 16).map(Number);
        return [
          m[0],
          -m[2],
          m[1],
          0,
          -m[8],
          m[10],
          -m[9],
          0,
          m[4],
          -m[6],
          m[5],
          0,
          m[12],
          -m[14],
          m[13],
          1
        ];
      }
      function visit(value, pathName, callback) {
        if (!value || typeof value !== "object") return;
        callback(value, pathName);
        if (Array.isArray(value)) {
          value.forEach((child, index) => visit(child, `${pathName}[${index}]`, callback));
        } else {
          for (const [name, child] of Object.entries(value)) visit(child, pathName ? `${pathName}/${name}` : name, callback);
        }
      }
      function halfToFloat(value) {
        const sign = value & 32768 ? -1 : 1;
        const exponent = value >>> 10 & 31;
        const fraction = value & 1023;
        if (exponent === 0) return sign * 2 ** -14 * (fraction / 1024);
        if (exponent === 31) return fraction ? Number.NaN : sign * Number.POSITIVE_INFINITY;
        return sign * 2 ** (exponent - 15) * (1 + fraction / 1024);
      }
      function formatInfo(format) {
        const normalized = String(format || "").toUpperCase();
        const match = /^R(\d+)(?:G(\d+))?(?:B(\d+))?(?:A(\d+))?_(UNORM|SNORM|FLOAT)$/.exec(normalized);
        if (!match) return null;
        const bits2 = match.slice(1, 5).filter(Boolean).map(Number);
        if (!bits2.length || bits2.some((value) => ![16, 32].includes(value))) return null;
        return { bits: bits2, type: match[5], bytes: bits2.reduce((sum, value) => sum + value, 0) / 8 };
      }
      function readAttributeComponent(buffer, byteOffset, bits2, type) {
        if (bits2 === 16 && type === "UNORM") return buffer.readUInt16LE(byteOffset) / 65535;
        if (bits2 === 16 && type === "SNORM") return Math.max(-1, buffer.readInt16LE(byteOffset) / 32767);
        if (bits2 === 16 && type === "FLOAT") return halfToFloat(buffer.readUInt16LE(byteOffset));
        if (bits2 === 32 && type === "FLOAT") return buffer.readFloatLE(byteOffset);
        if (bits2 === 32 && type === "UNORM") return buffer.readUInt32LE(byteOffset) / 4294967295;
        if (bits2 === 32 && type === "SNORM") return Math.max(-1, buffer.readInt32LE(byteOffset) / 2147483647);
        return Number.NaN;
      }
      function decodeAttribute(buffer, stride, vertex, descriptor, components) {
        const normalizedFormat = String(descriptor.Format || "").toUpperCase();
        const base = vertex * stride + Number(descriptor.ByteOffset || 0);
        const offset = Array.isArray(descriptor.Offset) ? descriptor.Offset : [];
        const scale = Array.isArray(descriptor.Scale) ? descriptor.Scale : [];
        if (normalizedFormat === "R21G21B22_UINT" && components === 3) {
          if (base < 0 || base + 8 > buffer.length) return null;
          const packed = buffer.readBigUInt64LE(base);
          const values = [
            Number(packed & 0x1fffffn),
            Number(packed >> 21n & 0x1fffffn),
            Number(packed >> 42n & 0x3fffffn)
          ];
          return values.map((value, index) => value * (Number.isFinite(scale[index]) ? scale[index] : 1) + (Number.isFinite(offset[index]) ? offset[index] : 0));
        }
        const info = formatInfo(descriptor.Format);
        if (!info || info.bits.length < components) return null;
        if (base < 0 || base + info.bytes > buffer.length) return null;
        const result = [];
        let cursor = base;
        for (let index = 0; index < components; index += 1) {
          let value = readAttributeComponent(buffer, cursor, info.bits[index], info.type);
          if (!Number.isFinite(value)) return null;
          value = value * (Number.isFinite(scale[index]) ? scale[index] : 1) + (Number.isFinite(offset[index]) ? offset[index] : 0);
          result.push(value);
          cursor += info.bits[index] / 8;
        }
        return result;
      }
      function vertexStreams(model) {
        const streams = model.VertexStream;
        if (Array.isArray(streams)) return streams;
        if (!streams || typeof streams !== "object") return [];
        if (streams.VertexBuffer && typeof streams.VertexBuffer === "object") return [streams.VertexBuffer];
        return Object.values(streams).filter((stream) => stream && typeof stream === "object" && stream.Binary);
      }
      function selectPrimitiveRange(primitive, preferHighestDetail = false) {
        if (Array.isArray(primitive.LodList)) {
          const start = preferHighestDetail ? 0 : primitive.LodList.length - 1;
          const end = preferHighestDetail ? primitive.LodList.length : -1;
          const step = preferHighestDetail ? 1 : -1;
          for (let index = start; index !== end; index += step) {
            const lod = primitive.LodList[index];
            if (Number.isInteger(lod?.Start) && Number.isInteger(lod?.Count) && lod.Count >= 3) return lod;
          }
        }
        if (Number.isInteger(primitive.Start) && Number.isInteger(primitive.Count) && primitive.Count >= 3) {
          return { Start: primitive.Start, Count: primitive.Count };
        }
        if (primitive.Start === void 0 && Number.isInteger(primitive.Count) && primitive.Count >= 3) {
          return { Start: 0, Count: primitive.Count };
        }
        return null;
      }
      function readIndices(buffer, format) {
        const normalized = String(format || "").toUpperCase();
        if (normalized === "R16_UINT") {
          const output = new Uint32Array(Math.floor(buffer.length / 2));
          for (let index = 0; index < output.length; index += 1) output[index] = buffer.readUInt16LE(index * 2);
          return output;
        }
        if (normalized === "R32_UINT") {
          const output = new Uint32Array(Math.floor(buffer.length / 4));
          for (let index = 0; index < output.length; index += 1) output[index] = buffer.readUInt32LE(index * 4);
          return output;
        }
        return null;
      }
      function memberFor(iff, name) {
        if (!name) return null;
        const normalized = String(name).replaceAll("\\", "/").toLowerCase();
        const direct = iff.lookup.get(normalized) || iff.lookup.get(path.posix.basename(normalized));
        if (direct || !normalized.endsWith(".gz")) return direct || null;
        const decoded = `${normalized.slice(0, -3)}.bin`;
        return iff.lookup.get(decoded) || iff.lookup.get(path.posix.basename(decoded)) || null;
      }
      function readMorphBits(buffer, bitOffset, width) {
        if (!Number.isInteger(width) || width < 0 || width > 31) throw new Error("invalid morph component width");
        if (!width) return { value: 0, bitOffset };
        if (!Number.isSafeInteger(bitOffset) || bitOffset < 0 || bitOffset + width > buffer.length * 8) {
          throw new Error("truncated morph component");
        }
        let value = 0;
        for (let bit = 0; bit < width; bit += 1) {
          value += (buffer[bitOffset + bit >>> 3] >>> (bitOffset + bit & 7) & 1) * 2 ** bit;
        }
        const sign = 2 ** (width - 1);
        return { value: value >= sign ? value - 2 ** width : value, bitOffset: bitOffset + width };
      }
      function decodeSparseMorph(buffer, descriptor, vertexCount, sourceVertexIds) {
        const bits2 = Array.isArray(descriptor?.Bits) ? descriptor.Bits.map(Number) : [];
        const scale = Number(descriptor?.Scale);
        if (!import_buffer.Buffer.isBuffer(buffer) || bits2.length !== 3 || bits2.some((value) => !Number.isInteger(value) || value < 0 || value > 31) || !bits2.some(Boolean) || !Number.isFinite(scale) || scale <= 0 || !Number.isInteger(vertexCount) || vertexCount < 0) {
          throw new Error("invalid morph descriptor");
        }
        const sourceLookup = /* @__PURE__ */ new Map();
        for (let index = 0; index < sourceVertexIds.length; index += 1) {
          const source = Number(sourceVertexIds[index]);
          if (!Number.isInteger(source) || source < 0 || source >= vertexCount) throw new Error("invalid morph source vertex");
          sourceLookup.set(source, index);
        }
        const deltas = new Float32Array(sourceVertexIds.length * 3);
        let byteOffset = 0;
        let vertex = 0;
        while (byteOffset < buffer.length) {
          if (byteOffset + 4 > buffer.length) throw new Error("truncated morph run");
          const skip = buffer.readUInt16LE(byteOffset);
          const count = buffer.readUInt16LE(byteOffset + 2);
          byteOffset += 4;
          vertex += skip;
          if (vertex > vertexCount || count > vertexCount - vertex) throw new Error("morph run exceeds vertex count");
          let bitOffset = byteOffset * 8;
          for (let runVertex = 0; runVertex < count; runVertex += 1) {
            const source = vertex + runVertex;
            const target = sourceLookup.get(source);
            const values = [];
            for (const width of bits2) {
              const decoded = readMorphBits(buffer, bitOffset, width);
              values.push(decoded.value / scale);
              bitOffset = decoded.bitOffset;
            }
            if (target !== void 0) {
              const offset = target * 3;
              deltas[offset] = values[0];
              deltas[offset + 1] = -values[2];
              deltas[offset + 2] = values[1];
            }
          }
          byteOffset = Math.ceil(bitOffset / 8);
          vertex += count;
        }
        if (byteOffset !== buffer.length || vertex !== vertexCount) throw new Error("incomplete morph vertex coverage");
        return deltas;
      }
      function decodeFitMorphTargets(iff, model, vertexCount, sourceVertexIds) {
        const names = [];
        const arrays = [];
        for (const [name, descriptor] of Object.entries(model.Morph || {})) {
          if (!/(?:THIN|THICK)$/i.test(name) || !descriptor?.Binary) continue;
          const entry = memberFor(iff, descriptor.Binary);
          if (!entry) continue;
          try {
            const bytes2 = readZipEntry(iff.buffer, entry, true);
            const expected = Number(descriptor.Size);
            if (Number.isInteger(expected) && expected >= 0 && bytes2.length !== expected) throw new Error("morph size mismatch");
            arrays.push(decodeSparseMorph(bytes2, descriptor, vertexCount, sourceVertexIds));
            names.push(name);
          } catch {
          }
        }
        if (!arrays.length) return { names: [], deltas: new Float32Array() };
        const stride = sourceVertexIds.length * 3;
        const deltas = new Float32Array(arrays.length * stride);
        arrays.forEach((values, index) => deltas.set(values, index * stride));
        return { names, deltas };
      }
      function textureBinding(name, source = "") {
        const value = path.posix.basename(String(name || "").replaceAll("\\", "/")).toLowerCase();
        const sourcePath = String(source || "").replaceAll("\\", "/").toLowerCase();
        const pcreateHairFins = /hair_underpaint_.+_colorhair_.+_fins_/.test(sourcePath);
        const pcreateHairUnderpaint = /(?:^|\/)hair_underpaint_/.test(sourcePath) && !pcreateHairFins;
        const pcreateFacialUnderpaint = /facialhair_.+_underpaint_/.test(sourcePath);
        const pcreateFacialHair = /(?:^|\/)(?:fh_|beard_|fullbeard_|fullgoatee_|full_scruff)/.test(sourcePath);
        const target = pcreateHairFins ? "hair" : pcreateHairUnderpaint ? "hair-underpaint" : pcreateFacialUnderpaint ? "facialhair-underpaint" : pcreateFacialHair ? "facialhair" : /eyewhite|iris_/.test(value) ? "eye" : /(?:^|_)lash(?:_|\.)/.test(value) ? "eyelash" : value.includes("facialhair") ? "facialhair" : /(?:^|_)face_/.test(value) ? "face" : /(?:^|_)hair_/.test(value) ? "hair" : /(?:^|_)chest_/.test(value) ? "chest" : /(?:^|_)torso_/.test(value) ? "torso" : /(?:^|_)legs_/.test(value) ? "legs" : value.startsWith("ball_") || /(?:^|\/)ball\/texture(?:\/|$)/.test(sourcePath) ? "ball" : "";
        const sourceChannel = /_fins_color\.iff$|_(?:beard_[^/]+|fullbeard_[^/]+|fullgoatee_[^/]+)_color\.iff$/.test(sourcePath) ? "color" : /_bump\.iff$|_underpaint_normal\.iff$/.test(sourcePath) ? "normal" : /_fins_(?:tangent|lightmap|occlusion)\.iff$|_(?:bentnormal|coneangle|hairmask)\.iff$/.test(sourcePath) ? "auxiliary" : "";
        const channel = sourceChannel || (/wrinkle|bentnormal|coneangle|hairmask|lightmap|tangent|occlusion/.test(value) ? "auxiliary" : /normal/.test(value) ? "normal" : /color|albedo|diffuse/.test(value) ? "color" : "auxiliary");
        return { target, channel };
      }
      function textureInputColorSpace(texture) {
        if (texture?.range) return "none";
        const channel = String(texture?.binding?.channel || "").toLowerCase();
        if (channel && channel !== "color") return "none";
        const format = String(texture?.format || "").trim().toUpperCase();
        if (format && format !== "DDS") return format.includes("SRGB") ? "srgb" : "none";
        return channel === "color" || /basecolou?r|(?:^|_)color(?:_|\.)|albedo|diffuse|_c(?:\.|_)/i.test(
          `${texture?.source || ""} ${texture?.name || ""}`
        ) ? "srgb" : "none";
      }
      function txtrRangeMetadata(metadata) {
        const min = Array.isArray(metadata?.Min) ? metadata.Min.slice(0, 4).map(Number) : [];
        const max2 = Array.isArray(metadata?.Max) ? metadata.Max.slice(0, 4).map(Number) : [];
        if (min.length < 4 || max2.length < 4 || [...min, ...max2].some((value) => !Number.isFinite(value))) return null;
        const rangeCompress = Number(metadata?.RangeCompress || 0);
        const mask = rangeCompress ? [0, 1, 2, 3].map((channel) => rangeCompress & 1 << channel ? 1 : 0) : [0, 1, 2, 3].map((channel) => Math.abs(min[channel]) > 1e-7 || Math.abs(max2[channel] - 1) > 1e-7 ? 1 : 0);
        return mask.some(Boolean) ? { min, max: max2, mask } : null;
      }
      function parseTxtrDocument(buffer) {
        const text = buffer.toString("utf8").replace(/^\uFEFF/, "").trim();
        if (text.startsWith("{") || text.startsWith("[")) return JSON.parse(text);
        if (/^"(?:[^"\\]|\\.)+"\s*:/.test(text)) return JSON.parse(`{${text}}`);
        return parseBinaryTree(buffer);
      }
      function textureTxtrMetadata(iff, textureName) {
        const leaf = path.posix.basename(String(textureName || "").replaceAll("\\", "/"));
        const logicalName = leaf.replace(/\.[0-9a-f]{16}\.dds$/i, "");
        if (logicalName === leaf) return null;
        const txtrEntry = memberFor(iff, `${logicalName}.TXTR`);
        if (!txtrEntry) return null;
        try {
          const tree = parseTxtrDocument(readZipEntry(iff.buffer, txtrEntry, true));
          const metadata = tree && Object.values(tree)[0];
          const result = { format: String(metadata?.Format || "") };
          result.range = txtrRangeMetadata(metadata);
          return result;
        } catch {
          return null;
        }
      }
      function collectTextureMetadata(scene, output, container = 0) {
        visit(scene, "", (node, nodePath) => {
          if (typeof node.Binary !== "string" || !/\.(?:dds|tld)$/i.test(node.Binary)) return;
          const decodedName = node.Binary.replace(/\.tld$/i, ".dds");
          const key = `${container}:${decodedName.toLowerCase()}`;
          if (output.has(key)) return;
          output.set(key, {
            name: decodedName,
            source: nodePath,
            width: Number(node.Width || 0),
            height: Number(node.Height || 0),
            mips: Number(node.Mips || 0),
            format: String(node.Format || "DDS"),
            binding: textureBinding(node.Binary, nodePath),
            container
          });
        });
      }
      function collectTextureReferences(scene, output, container = 0) {
        visit(scene, "", (node) => {
          if (typeof node.Binary !== "string" || !/\.(?:dds|tld)$/i.test(node.Binary)) return;
          output.add(`${container}:${node.Binary.replace(/\.tld$/i, ".dds").toLowerCase()}`);
        });
      }
      function arenaTextureChannel(resourcePath) {
        const value = String(resourcePath || "").toLowerCase();
        if (/albedo|diffuse|basecolou?r|colou?rmap/.test(value)) return "color";
        if (/normal/.test(value)) return "normal";
        if (/rough/.test(value)) return "roughness";
        if (/metal/.test(value)) return "metalness";
        if (/emissive/.test(value)) return "emissive";
        return "";
      }
      function collectArenaMaterialTextures(scene, output, container = 0) {
        for (const root of findSceneRoots(scene)) {
          const textureLookup = new Map(Object.entries(root.Texture || {}).map(([name, texture]) => [name.toLowerCase(), texture]));
          for (const [material, definition] of Object.entries(root.Material || {})) {
            visit(definition, "", (node, nodePath) => {
              if (typeof node?.Pixelmap !== "string") return;
              const channel = arenaTextureChannel(nodePath);
              if (!channel) return;
              const texture = textureLookup.get(node.Pixelmap.toLowerCase());
              if (!texture || typeof texture.Binary !== "string" || !/\.(?:dds|tld)$/i.test(texture.Binary)) return;
              const decodedName = texture.Binary.replace(/\.tld$/i, ".dds");
              const key = `${container}:${decodedName.toLowerCase()}`;
              let item = output.get(key);
              if (!item) {
                item = {
                  name: decodedName,
                  source: node.Pixelmap,
                  width: Number(texture.Width || 0),
                  height: Number(texture.Height || 0),
                  mips: Number(texture.Mips || 0),
                  format: String(texture.Format || "DDS"),
                  binding: { target: "arena", channel },
                  assignments: [],
                  container
                };
                output.set(key, item);
              }
              if (!item.assignments.some((assignment) => assignment.material === material && assignment.channel === channel)) {
                item.assignments.push({ material, channel });
              }
            });
          }
        }
      }
      function previewTextureUnused(item) {
        const identity = `${item.name || ""} ${item.source || ""}`;
        const channels = item.assignments?.length ? item.assignments.map((assignment) => assignment.channel) : [item.binding?.channel];
        if (channels.some((channel) => channel === "color" || channel === "tangent")) return false;
        if (channels.includes("normal")) return /detailnormal/i.test(identity) && (!/^face_detailnormal_o\./i.test(item.name || "") || /_skin\.iff/i.test(identity)) || /_normal_avg_/i.test(identity) && !item.externalPath;
        return !/occlusion|(?:^|_)ao(?:_|\.)|lightmap|tangent/i.test(identity);
      }
      function previewTextureScore(item) {
        const channels = item.assignments?.map((assignment) => assignment.channel) || [item.binding?.channel];
        const channelScore = channels.includes("color") ? 10 : channels.includes("normal") ? 5 : 1;
        const identity = `${item.name || ""} ${item.source || ""}`;
        const hairTarget = String(item.binding?.target || "").toLowerCase();
        const hairDetailScore = hairTarget === "hair" || hairTarget === "facialhair" ? /tangent/i.test(identity) ? 9 : /lightmap|occlusion/i.test(identity) ? 8 : 0 : 0;
        const courtScore = /court|floor|hardwood|basket|scoreboard|scorecube/i.test(`${item.name} ${item.source}`) ? 8 : 0;
        const target = String(item.binding?.target || "").toLowerCase();
        const playerColorScore = channels.includes("color") ? ["chest", "torso", "legs"].includes(target) ? 80 : target === "face" ? 70 : ["eye", "hair", "facialhair", "eyelash"].includes(target) ? 50 : 0 : 0;
        const playerNormalScore = channels.includes("normal") && ["face", "chest", "torso", "legs"].includes(target) && !/detailnormal/i.test(identity) ? 20 : 0;
        return channelScore + hairDetailScore + courtScore + playerColorScore + playerNormalScore;
      }
      function comparePreviewTextures(a, b) {
        return previewTextureScore(b) - previewTextureScore(a) || (bodyColorTextureTarget(a) && bodyColorTextureTarget(b) ? bodyColorRank(a) - bodyColorRank(b) : 0) || (b.assignments?.length || 0) - (a.assignments?.length || 0) || a.width * a.height - b.width * b.height;
      }
      function bodyColorRank(texture) {
        return /^config:/i.test(String(texture.source || "")) ? -1 : texture.container ?? Number.MAX_SAFE_INTEGER;
      }
      function bodyColorTextureTarget(texture) {
        const target = String(texture.binding?.target || "").toLowerCase();
        return !texture.assignments?.length && texture.binding?.channel === "color" && ["chest", "torso", "legs"].includes(target) ? target : null;
      }
      function primitiveMaterial(primitive, inherited = "Unassigned") {
        const declared = String(primitive?.Material || "").trim();
        return declared || inherited || "Unassigned";
      }
      function modelBones(model) {
        return Object.entries(model.Transform || {}).filter(([, value]) => value && typeof value === "object" && !Array.isArray(value)).map(([name, value]) => ({
          name,
          parent: Number.isInteger(value.Parent) ? value.Parent : -1,
          translate: Array.isArray(value.Translate) ? value.Translate.slice(0, 3).map((item) => Number(item) || 0) : [0, 0, 0]
        }));
      }
      function vertexSkinData(iff, model, streams) {
        const descriptor = model.VertexFormat?.WEIGHTDATA0;
        const stream = descriptor ? streams[Number(descriptor.Stream || 0)] : null;
        const streamEntry = memberFor(iff, stream?.Binary);
        const matrixEntry = memberFor(iff, model.MatrixWeightsBuffer?.Binary);
        const stride = Number(stream?.Stride || 0);
        const byteOffset = Number(descriptor?.ByteOffset || 0);
        if (String(descriptor?.Format || "").toUpperCase() !== "R32_UINT" || !streamEntry || !matrixEntry || stride <= 0 || byteOffset < 0 || byteOffset + 4 > stride) return null;
        return {
          bytes: readZipEntry(iff.buffer, streamEntry, true),
          matrix: readZipEntry(iff.buffer, matrixEntry, true),
          stride,
          byteOffset
        };
      }
      function decodeVertexSkin(skin, vertex, boneCount) {
        if (!skin) return [];
        const offset = vertex * skin.stride + skin.byteOffset;
        if (offset < 0 || offset + 4 > skin.bytes.length) return [];
        const packed = skin.bytes.readUInt32LE(offset);
        const extraWeights = packed & 255;
        const index = packed >>> 8;
        if (!extraWeights) return index < boneCount ? [{ bone: index, weight: 1 }] : [];
        const count = extraWeights + 1;
        if ((index + count) * 4 > skin.matrix.length) return [];
        const influences = [];
        let total = 0;
        for (let cursor = 0; cursor < count; cursor += 1) {
          const encoded = skin.matrix.readUInt32LE((index + cursor) * 4);
          const bone = encoded >>> 16;
          const weight = (encoded & 65535) / 65535;
          if (bone >= boneCount || weight <= 0) continue;
          influences.push({ bone, weight });
          total += weight;
        }
        if (total > 0 && Math.abs(total - 1) > 1e-5) {
          influences.forEach((item) => {
            item.weight /= total;
          });
        }
        return influences;
      }
      function buildMesh(iff, model, modelPath, remainingTriangles, materialUsage, preferHighestDetail = false) {
        const position = model.VertexFormat?.POSITION0;
        const tangentSpaceAttachment = String(position?.Format || "").toUpperCase() === "R16G16B16A16_UINT" && Boolean(model.VertexFormat?.POSITION1 && model.VertexFormat?.TEXCOORD0 && model.VertexFormat?.TEXCOORD1);
        const strandCards = !tangentSpaceAttachment && Boolean(model.VertexFormat?.TEXCOORD1) && (() => {
          let inherited = "";
          const materials = (Array.isArray(model.Prim) ? model.Prim : []).map((primitive) => inherited = primitiveMaterial(primitive, inherited));
          return materials.length > 0 && materials.every((material) => /^(?:facial)?hair_/i.test(material));
        })();
        const streams = vertexStreams(model);
        const positionStream = streams[Number(position?.Stream || 0)];
        const indexEntry = memberFor(iff, model.IndexBuffer?.Binary);
        const positionEntry = memberFor(iff, positionStream?.Binary);
        if (!position || !positionStream || !indexEntry || !positionEntry) return { reason: "missing position or index buffer" };
        const positionFormat = formatInfo(position.Format);
        const stride = Number(positionStream.Stride || 0);
        if ((!positionFormat || positionFormat.bits.length < 3) && String(position.Format).toUpperCase() !== "R21G21B22_UINT" && !tangentSpaceAttachment) {
          return { reason: `unsupported position format ${position.Format || "(none)"}` };
        }
        if (stride <= 0) return { reason: "invalid position-stream stride" };
        const positionBytes = readZipEntry(iff.buffer, positionEntry, true);
        const indexBytes = readZipEntry(iff.buffer, indexEntry, true);
        const sourceIndices = readIndices(indexBytes, model.IndexBuffer.Format);
        if (!sourceIndices) return { reason: `unsupported index format ${model.IndexBuffer.Format || "(none)"}` };
        const vertexCount = Math.floor(positionBytes.length / stride);
        const uvDescriptors = tangentSpaceAttachment ? [model.VertexFormat?.TEXCOORD1, model.VertexFormat?.TEXCOORD0] : strandCards ? [model.VertexFormat?.TEXCOORD0, model.VertexFormat?.TEXCOORD1] : [model.VertexFormat?.TEXCOORD0];
        const uvOptions = uvDescriptors.filter(Boolean).map((descriptor) => {
          const stream = streams[Number(descriptor.Stream || 0)];
          const entry = memberFor(iff, stream?.Binary);
          const bytes2 = entry ? readZipEntry(iff.buffer, entry, true) : null;
          const streamStride = Number(stream?.Stride || 0);
          let score = -1;
          if (bytes2 && streamStride > 0 && vertexCount > 0) {
            const minimum = [Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY];
            const maximum = [Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY];
            const sampleCount = Math.min(vertexCount, 128);
            let decodedCount = 0;
            for (let sample = 0; sample < sampleCount; sample += 1) {
              const vertex = sampleCount === 1 ? 0 : Math.floor(sample * (vertexCount - 1) / (sampleCount - 1));
              const decoded = decodeAttribute(bytes2, streamStride, vertex, descriptor, 2);
              if (!decoded) continue;
              decodedCount += 1;
              for (let axis = 0; axis < 2; axis += 1) {
                minimum[axis] = Math.min(minimum[axis], decoded[axis]);
                maximum[axis] = Math.max(maximum[axis], decoded[axis]);
              }
            }
            if (decodedCount) score = maximum[0] - minimum[0] + (maximum[1] - minimum[1]);
          }
          return { descriptor, stream, bytes: bytes2, stride: streamStride, score };
        });
        if (tangentSpaceAttachment) uvOptions.sort((left, right) => right.score - left.score);
        const uvOption = uvOptions[0] || {};
        const uvDescriptor = uvOption.descriptor;
        const uvBytes = uvOption.bytes || null;
        const uvStride = Number(uvOption.stride || 0);
        const attachmentOption = tangentSpaceAttachment || strandCards ? uvOptions.find((option) => option.descriptor !== uvDescriptor) || {} : {};
        const attachmentDescriptor = attachmentOption.descriptor;
        const attachmentBytes = attachmentOption.bytes || null;
        const attachmentStride = Number(attachmentOption.stride || 0);
        const primitives = Array.isArray(model.Prim) ? model.Prim : [];
        const bones = modelBones(model);
        const sourceSkin = vertexSkinData(iff, model, streams);
        const remap = /* @__PURE__ */ new Map();
        const positions = [];
        const sourceVertexIds = [];
        const uvs = [];
        const attachmentUvs = [];
        const tangentOffsets = [];
        const tangentAnchorIds = [];
        const indices = [];
        const skinOffsets = [0];
        const skinBones = [];
        const skinWeights = [];
        const groups = [];
        let triangles = 0;
        let inheritedMaterial = "Unassigned";
        for (const primitive of primitives) {
          inheritedMaterial = primitiveMaterial(primitive, inheritedMaterial);
          const range = selectPrimitiveRange(primitive, preferHighestDetail);
          if (!range || triangles >= remainingTriangles) continue;
          const available = Math.min(range.Count, sourceIndices.length - range.Start);
          const count = Math.min(available - available % 3, (remainingTriangles - triangles) * 3);
          if (count < 3 || range.Start < 0) continue;
          const groupStart = indices.length;
          for (let cursor = 0; cursor < count; cursor += 3) {
            const triangle = [sourceIndices[range.Start + cursor], sourceIndices[range.Start + cursor + 1], sourceIndices[range.Start + cursor + 2]];
            if (triangle.some((vertex) => vertex >= vertexCount)) continue;
            for (const sourceVertex of triangle) {
              let targetVertex = remap.get(sourceVertex);
              if (targetVertex === void 0) {
                const decodedPosition = tangentSpaceAttachment ? [0, 0, 0] : decodeAttribute(positionBytes, stride, sourceVertex, position, 3);
                if (!decodedPosition) return { reason: `could not decode ${position.Format}` };
                targetVertex = remap.size;
                remap.set(sourceVertex, targetVertex);
                sourceVertexIds.push(sourceVertex);
                positions.push(decodedPosition[0], -decodedPosition[2], decodedPosition[1]);
                const decodedUv = uvBytes && uvStride > 0 ? decodeAttribute(uvBytes, uvStride, sourceVertex, uvDescriptor, 2) : null;
                uvs.push(decodedUv?.[0] || 0, decodedUv?.[1] || 0);
                const decodedAttachment = attachmentBytes && attachmentStride > 0 ? decodeAttribute(attachmentBytes, attachmentStride, sourceVertex, attachmentDescriptor, 2) : null;
                attachmentUvs.push(decodedAttachment?.[0] || 0, decodedAttachment?.[1] || 0);
                if (tangentSpaceAttachment) {
                  const packedBase = sourceVertex * stride + Number(position.ByteOffset || 0);
                  tangentOffsets.push(
                    halfToFloat(positionBytes.readUInt16LE(packedBase)),
                    halfToFloat(positionBytes.readUInt16LE(packedBase + 2)),
                    halfToFloat(positionBytes.readUInt16LE(packedBase + 4))
                  );
                  tangentAnchorIds.push(positionBytes.readUInt16LE(packedBase + 6));
                }
                for (const influence of decodeVertexSkin(sourceSkin, sourceVertex, bones.length)) {
                  skinBones.push(influence.bone);
                  skinWeights.push(influence.weight);
                }
                skinOffsets.push(skinBones.length);
              }
              indices.push(targetVertex);
            }
          }
          const groupCount = indices.length - groupStart;
          if (groupCount) {
            const material = inheritedMaterial;
            groups.push({ start: groupStart, count: groupCount, material, mesh: String(primitive.Mesh || "") });
            materialUsage.set(material, (materialUsage.get(material) || 0) + 1);
            triangles += groupCount / 3;
          }
        }
        if (!indices.length) return { reason: "no triangle-list primitive at the selected LOD" };
        const fitMorphs = /^static(?:jersey|shorts)/i.test(modelPath) ? decodeFitMorphTargets(iff, model, vertexCount, sourceVertexIds) : { names: [], deltas: new Float32Array() };
        return {
          mesh: {
            name: modelPath.split("/").at(-1) || modelPath,
            positions: new Float32Array(positions),
            sourceVertexIds: new Uint32Array(sourceVertexIds),
            uvs: new Float32Array(uvs),
            attachmentUvs: tangentSpaceAttachment || strandCards ? new Float32Array(attachmentUvs) : null,
            tangentOffsets: tangentSpaceAttachment ? new Float32Array(tangentOffsets) : null,
            tangentAnchorIds: tangentSpaceAttachment ? new Uint32Array(tangentAnchorIds) : null,
            tangentSpaceAttachment,
            indices: new Uint32Array(indices),
            groups,
            bones,
            skinOffsets: new Uint32Array(skinOffsets),
            skinBones: new Uint16Array(skinBones),
            skinWeights: new Float32Array(skinWeights),
            fitMorphNames: fitMorphs.names,
            fitMorphDeltas: fitMorphs.deltas,
            bounds: { min: model.Min || null, max: model.Max || null, center: model.Center || null }
          },
          triangles
        };
      }
      function buildHairFitBust(mesh, headShaderOnly = false) {
        const headPattern = headShaderOnly ? /(?:^|_)face_shader(?:_|$)/i : /face|head|(?:^|_)ears?(?:_|$)|eye|sclera|iris|tear|caruncle|mouth|tongue|teeth|gum|lash/i;
        const accessoryPattern = /hair|beard|headband/i;
        const bodyPattern = /torso|arms?|chest|body|hidden|neck|shoulder/i;
        const headVertices = /* @__PURE__ */ new Set();
        const allVertices = /* @__PURE__ */ new Set();
        for (const group of mesh.groups || []) {
          for (let cursor = group.start; cursor < group.start + group.count; cursor += 1) {
            const vertex = mesh.indices[cursor];
            allVertices.add(vertex);
            if (headPattern.test(group.material) && !accessoryPattern.test(group.material)) headVertices.add(vertex);
          }
        }
        const sourceVertices = headVertices.size ? headVertices : headShaderOnly ? /* @__PURE__ */ new Set() : allVertices;
        if (!sourceVertices.size) return null;
        let minZ = Number.POSITIVE_INFINITY;
        let maxZ = Number.NEGATIVE_INFINITY;
        let minX = Number.POSITIVE_INFINITY;
        let maxX = Number.NEGATIVE_INFINITY;
        for (const vertex of sourceVertices) {
          const offset = vertex * 3;
          minX = Math.min(minX, mesh.positions[offset]);
          maxX = Math.max(maxX, mesh.positions[offset]);
          minZ = Math.min(minZ, mesh.positions[offset + 2]);
          maxZ = Math.max(maxZ, mesh.positions[offset + 2]);
        }
        const headHeight = Math.max(1, maxZ - minZ);
        const headWidth = Math.max(1, maxX - minX);
        const cutoffZ = minZ - Math.max(8, headHeight * 0.32);
        const centerX = (minX + maxX) * 0.5;
        const shoulderHalfWidth = Math.max(headWidth, 18);
        const remap = /* @__PURE__ */ new Map();
        const positions = [];
        const sourceVertexIds = [];
        const uvs = [];
        const indices = [];
        const skinOffsets = [0];
        const skinBones = [];
        const skinWeights = [];
        const groups = [];
        const appendVertex = (sourceVertex) => {
          const existing = remap.get(sourceVertex);
          if (existing !== void 0) return existing;
          const targetVertex = remap.size;
          remap.set(sourceVertex, targetVertex);
          positions.push(
            mesh.positions[sourceVertex * 3],
            mesh.positions[sourceVertex * 3 + 1],
            mesh.positions[sourceVertex * 3 + 2]
          );
          sourceVertexIds.push(mesh.sourceVertexIds?.[sourceVertex] ?? sourceVertex);
          uvs.push(mesh.uvs[sourceVertex * 2] || 0, mesh.uvs[sourceVertex * 2 + 1] || 0);
          const skinStart = mesh.skinOffsets[sourceVertex] || 0;
          const skinEnd = mesh.skinOffsets[sourceVertex + 1] || skinStart;
          for (let cursor = skinStart; cursor < skinEnd; cursor += 1) {
            skinBones.push(mesh.skinBones[cursor]);
            skinWeights.push(mesh.skinWeights[cursor]);
          }
          skinOffsets.push(skinBones.length);
          return targetVertex;
        };
        for (const group of mesh.groups || []) {
          const keepHead = headPattern.test(group.material) && !accessoryPattern.test(group.material);
          const keepBody = !headShaderOnly && bodyPattern.test(group.material) && !/hair/i.test(group.material);
          if (!keepHead && !keepBody) continue;
          const groupStart = indices.length;
          for (let cursor = group.start; cursor < group.start + group.count; cursor += 3) {
            const triangle = [mesh.indices[cursor], mesh.indices[cursor + 1], mesh.indices[cursor + 2]];
            if (!keepHead) {
              const maxTriangleZ = Math.max(...triangle.map((vertex) => mesh.positions[vertex * 3 + 2]));
              const centerTriangleX = triangle.reduce((sum, vertex) => sum + mesh.positions[vertex * 3], 0) / 3;
              if (maxTriangleZ < cutoffZ || Math.abs(centerTriangleX - centerX) > shoulderHalfWidth) continue;
            }
            triangle.forEach((vertex) => indices.push(appendVertex(vertex)));
          }
          const groupCount = indices.length - groupStart;
          if (groupCount) groups.push({ ...group, start: groupStart, count: groupCount });
        }
        if (!indices.length) return null;
        return {
          ...mesh,
          positions: new Float32Array(positions),
          sourceVertexIds: new Uint32Array(sourceVertexIds),
          uvs: new Float32Array(uvs),
          indices: new Uint32Array(indices),
          groups,
          skinOffsets: new Uint32Array(skinOffsets),
          skinBones: new Uint16Array(skinBones),
          skinWeights: new Float32Array(skinWeights)
        };
      }
      var { projectTangentSpaceMeshes: projectTangentSpaceMeshes3 } = (init_tangent_frame(), __toCommonJS(tangent_frame_exports));
      function modelGeometrySignature(model) {
        let inheritedMaterial = "Unassigned";
        const primitives = (Array.isArray(model.node?.Prim) ? model.node.Prim : []).map((primitive) => {
          inheritedMaterial = primitiveMaterial(primitive, inheritedMaterial);
          const range = selectPrimitiveRange(primitive, model.preferHighestDetail);
          return [inheritedMaterial, range?.Start ?? null, range?.Count ?? null];
        });
        const streams = (Array.isArray(model.node?.VertexStream) ? model.node.VertexStream : [model.node?.VertexStream]).filter(Boolean).map((stream) => [String(stream.Binary || "").toLowerCase(), Number(stream.Stride || 0)]);
        const attribute = (descriptor) => descriptor ? [String(descriptor.Format || "").toUpperCase(), Number(descriptor.Stream || 0), Number(descriptor.ByteOffset || 0)] : null;
        return JSON.stringify([
          model.container,
          String(model.node?.IndexBuffer?.Binary || "").toLowerCase(),
          String(model.node?.IndexBuffer?.Format || "").toUpperCase(),
          streams,
          attribute(model.node?.VertexFormat?.POSITION0),
          attribute(model.node?.VertexFormat?.TEXCOORD0),
          attribute(model.node?.VertexFormat?.WEIGHTDATA0),
          String(model.node?.MatrixWeightsBuffer?.Binary || "").toLowerCase(),
          primitives,
          Boolean(model.preferHighestDetail)
        ]);
      }
      function deduplicateModels(models) {
        const unique = [];
        const byGeometry = /* @__PURE__ */ new Map();
        for (const model of models) {
          const signature = modelGeometrySignature(model);
          const existing = byGeometry.get(signature);
          if (!existing) {
            const copy = { ...model, placements: [...model.placements], duplicateDefinitions: 0 };
            copy.placementKeys = new Set(copy.placements.map((placement) => JSON.stringify([
              placement.name,
              placement.target,
              placement.matrix
            ])));
            byGeometry.set(signature, copy);
            unique.push(copy);
            continue;
          }
          existing.duplicateDefinitions += 1;
          for (const placement of model.placements) {
            const key = JSON.stringify([placement.name, placement.target, placement.matrix]);
            if (existing.placementKeys.has(key)) continue;
            existing.placementKeys.add(key);
            existing.placements.push(placement);
          }
        }
        unique.forEach((model) => {
          delete model.placementKeys;
        });
        return unique;
      }
      function shouldHideArenaModel(model, arenaLike = true) {
        return Boolean(arenaLike && /debug_helper_sphere/i.test(String(model?.path || "")));
      }
      async function createIffPreview3(filePath, options = {}) {
        const iff = await loadIff(filePath);
        const companionSources = await Promise.all((options.companionPaths || []).map(async (companion) => ({
          name: companion.name || path.basename(companion.path),
          kind: companion.kind || "companion",
          iff: await loadIff(companion.path)
        })));
        const textureSources = [{ name: path.basename(filePath), kind: "main", iff }, ...companionSources];
        const maxTriangles = Math.max(1e3, Math.min(1e6, options.maxTriangles || DEFAULT_MAX_TRIANGLES));
        const maxModels = Math.max(1, Math.min(1e3, options.maxModels || DEFAULT_MAX_MODELS));
        const scenes = [];
        const models = [];
        const textures = /* @__PURE__ */ new Map();
        const arenaTextureCandidates = /* @__PURE__ */ new Map();
        const textureReferences = /* @__PURE__ */ new Set();
        const arenaByName = /^arena_.+_int\.iff$/i.test(path.basename(filePath));
        const includeArenaTextures = arenaByName && Boolean(options.includeArenaTextures);
        const skipTextures = Boolean(options.skipTextures);
        let objectMatrices = 0;
        let morphTargets = 0;
        let highDetailModels = 0;
        for (const [container, source] of textureSources.entries()) {
          if (!["main", "geometry", "preview-jersey"].includes(source.kind)) continue;
          const sourceScenes = source.iff.entries.filter((candidate) => /\.scne$/i.test(candidate.name));
          const staticGarmentScenes = source.kind === "preview-jersey" ? sourceScenes.filter((candidate) => /^static(?:jersey|shorts)/i.test(candidate.name)) : [];
          const previewScenes = staticGarmentScenes.length ? staticGarmentScenes : sourceScenes;
          for (const entry of previewScenes) {
            try {
              const scene = parseScneText(readZipEntry(source.iff.buffer, entry, true));
              scenes.push(container ? `${source.name}: ${entry.name}` : entry.name);
              if (!skipTextures) {
                if (arenaByName) collectTextureReferences(scene, textureReferences, container);
                if (includeArenaTextures) collectArenaMaterialTextures(scene, textures, container);
                else if (arenaByName) collectArenaMaterialTextures(scene, arenaTextureCandidates, container);
                else collectTextureMetadata(scene, textures, container);
              }
              const transforms = buildSceneTransformLookup(scene);
              objectMatrices += transforms.objectMatrices;
              visit(scene, entry.name, (node, nodePath) => {
                if (node?.IndexBuffer?.Binary && node.VertexStream && node?.VertexFormat?.POSITION0 && Array.isArray(node.Prim)) {
                  const placements = lookupScenePlacements(nodePath, node, transforms.lookup);
                  const modelMorphTargets = Object.keys(node.Morph || {}).length;
                  const preferHighestDetail = Boolean(options.preferHighestDetail) || modelMorphTargets > 0 || /(?:hihead|blend_head|basebody)/i.test(nodePath);
                  morphTargets += modelMorphTargets;
                  if (preferHighestDetail) highDetailModels += 1;
                  models.push({
                    container,
                    iff: source.iff,
                    node,
                    path: nodePath,
                    placements,
                    preferHighestDetail,
                    sourceKind: source.kind,
                    sourceName: source.name
                  });
                }
              });
            } catch {
            }
          }
        }
        const arenaTextureLeafKeys = /* @__PURE__ */ new Map();
        if (includeArenaTextures) {
          for (const [key, texture] of textures) {
            arenaTextureLeafKeys.set(`${texture.container}:${path.posix.basename(texture.name).toLowerCase()}`, key);
          }
        }
        if (!skipTextures) textureSources.forEach((source, container) => {
          for (const entry of source.iff.entries.filter((candidate) => /\.dds$/i.test(candidate.name))) {
            if (arenaByName && !includeArenaTextures) {
              textureReferences.add(`${container}:${entry.name.toLowerCase()}`);
              continue;
            }
            const key = `${container}:${entry.name.toLowerCase()}`;
            if (includeArenaTextures) {
              const mappedKey = textures.has(key) ? key : arenaTextureLeafKeys.get(`${container}:${path.posix.basename(entry.name).toLowerCase()}`);
              if (!mappedKey || textures.has(mappedKey)) continue;
            }
            if (textures.has(key)) continue;
            textures.set(key, {
              name: entry.name,
              source: `${source.kind}: ${source.name}`,
              width: 0,
              height: 0,
              mips: 0,
              format: "DDS",
              binding: textureBinding(entry.name, source.name),
              container
            });
          }
        });
        for (const [index, texture] of (skipTextures ? [] : options.externalTextures || []).entries()) {
          if (!texture?.path || !texture?.name) continue;
          textures.set(`external:${index}:${texture.name.toLowerCase()}`, {
            name: texture.name,
            source: texture.source || "shared texture cache",
            width: 0,
            height: 0,
            mips: 0,
            format: texture.format || "DDS",
            range: texture.range || null,
            binding: texture.binding || textureBinding(texture.name),
            externalPath: texture.path,
            container: null
          });
        }
        const meshes = options.nativeGeometry?.meshes || [];
        const skipped = textureSources.flatMap((s) => s.iff.warnings || []);
        const materialUsage = /* @__PURE__ */ new Map();
        let triangles = options.nativeGeometry?.stats.triangles || 0;
        meshes.forEach((mesh) => (mesh.groups || []).forEach((group) => materialUsage.set(group.material, (materialUsage.get(group.material) || 0) + 1)));
        const arenaLike = arenaByName || objectMatrices >= 20;
        const visibleModels = models.filter((model) => !shouldHideArenaModel(model, arenaLike));
        const hiddenModels = models.length - visibleModels.length;
        const uniqueModels = deduplicateModels(visibleModels);
        for (const model of options.nativeGeometry ? [] : uniqueModels.slice(0, maxModels)) {
          if (triangles >= maxTriangles) break;
          try {
            const result = buildMesh(
              model.iff,
              model.node,
              model.path,
              maxTriangles - triangles,
              materialUsage,
              model.preferHighestDetail
            );
            if (result.mesh) {
              if (options.hairFitBust || options.hairFitHeadOnly) {
                result.mesh = buildHairFitBust(result.mesh, Boolean(options.hairFitHeadOnly));
                result.triangles = result.mesh ? result.mesh.indices.length / 3 : 0;
              }
              if (!result.mesh) continue;
              result.mesh.sourceKind = model.sourceKind;
              result.mesh.geometryKey = model.iff.geometryKey ? model.iff.geometryKey + ":" + model.path : null;
              result.mesh.sourceName = model.sourceName;
              if (arenaLike && model.placements.length) {
                result.mesh.instanceMatrices = new Float32Array(model.placements.length * 16);
                result.mesh.instanceNames = new Array(model.placements.length);
                result.mesh.instanceTargets = new Array(model.placements.length);
                model.placements.forEach((placement, index) => {
                  result.mesh.instanceMatrices.set(gameMatrixToPreviewMatrix(placement.matrix), index * 16);
                  result.mesh.instanceNames[index] = placement.name;
                  result.mesh.instanceTargets[index] = placement.target;
                });
              } else {
                result.mesh.instances = model.placements.map((placement) => ({
                  name: placement.name,
                  target: placement.target,
                  matrix: gameMatrixToPreviewMatrix(placement.matrix)
                }));
              }
              meshes.push(result.mesh);
              triangles += result.triangles;
            } else if (skipped.length < 30) skipped.push(`${model.path}: ${result.reason}`);
          } catch (error) {
            if (skipped.length < 30) skipped.push(`${model.path}: ${error.message}`);
          }
        }
        const textureList = [...textures.values()];
        for (const texture of textureList) {
          const metadata = texture.container === null ? null : textureTxtrMetadata(textureSources[texture.container].iff, texture.name);
          if (metadata?.format) texture.format = metadata.format;
          texture.range = metadata?.range || texture.range || null;
        }
        const prioritized = [...textureList].sort(comparePreviewTextures);
        const textureDependencies = textureList.map((texture) => ({
          name: texture.name,
          source: texture.source,
          binding: texture.binding,
          format: texture.format,
          sourceIff: texture.container === null ? texture.source : textureSources[texture.container]?.name,
          bytes: texture.container === null ? null : memberFor(textureSources[texture.container].iff, texture.name)?.size || null,
          loaded: false
        }));
        const texturePayloads = [];
        const loadedBodyColors = /* @__PURE__ */ new Set();
        let textureBytes = 0;
        const maxTextureBytes = includeArenaTextures ? 64 * 1024 * 1024 : MAX_TEXTURE_BYTES;
        const maxTextureCount = includeArenaTextures ? 128 : 40;
        for (const texture of prioritized) {
          if (!includeArenaTextures && previewTextureUnused(texture)) continue;
          const bodyColorTarget = bodyColorTextureTarget(texture);
          if (bodyColorTarget && loadedBodyColors.has(bodyColorTarget)) continue;
          try {
            let bytes2;
            if (texture.externalPath) {
              bytes2 = await fsp.readFile(texture.externalPath);
            } else {
              const source = textureSources[texture.container];
              const entry = memberFor(source.iff, texture.name);
              if (!entry || entry.size > 8 * 1024 * 1024) continue;
              bytes2 = readZipEntry(source.iff.buffer, entry, true);
            }
            if (bytes2.length > 8 * 1024 * 1024 || textureBytes + bytes2.length > maxTextureBytes) continue;
            const { container, externalPath, ...publicTexture } = texture;
            texturePayloads.push({
              ...publicTexture,
              sourcePriority: Number.isInteger(container) ? container : Number.MAX_SAFE_INTEGER,
              inputColorSpace: textureInputColorSpace(publicTexture),
              bytes: bytes2.toString("base64")
            });
            textureBytes += bytes2.length;
            if (bodyColorTarget) loadedBodyColors.add(bodyColorTarget);
            if (texturePayloads.length >= maxTextureCount) break;
          } catch {
          }
        }
        const instanceCount = meshes.reduce((count, mesh) => {
          const packed = Math.floor((mesh.instanceMatrices?.length || 0) / 16);
          return count + Math.max(1, packed || mesh.instances?.length || 0);
        }, 0);
        if (morphTargets > 0) {
          skipped.unshift(`${morphTargets} morph target(s) detected (facial expressions and pose correctives). The preview shows the neutral rest pose, which already carries the player's likeness.`);
        }
        const arenaTextureRequests = arenaByName && !includeArenaTextures ? [...arenaTextureCandidates.values()].sort(comparePreviewTextures).slice(0, MAX_ARENA_TEXTURE_REQUESTS).map((texture) => texture.name.replace(/\.dds$/i, ".tld")) : [];
        const texturedMaterials = new Set(texturePayloads.flatMap((texture) => (texture.assignments || []).map((assignment) => String(assignment.material || "").toLowerCase())).filter(Boolean)).size;
        return {
          file: filePath,
          scenes,
          geometryKey: textureSources.every((s) => s.iff.geometryKey) ? textureSources.filter((s) => ["main", "geometry", "preview-jersey"].includes(s.kind)).map((s) => s.kind + ":" + s.iff.geometryKey).join("|") : null,
          textureKey: textureSources.every((s) => s.iff.contentKey) ? textureSources.map((s) => s.iff.contentKey).join("|") + JSON.stringify(options.externalTextures || []) : null,
          meshes,
          stats: {
            modelsFound: models.length,
            modelsHidden: hiddenModels,
            modelsUnique: uniqueModels.length,
            modelsDeduplicated: visibleModels.length - uniqueModels.length,
            modelsDecoded: meshes.length,
            instances: instanceCount,
            objectMatrices,
            triangles,
            morphTargets,
            highDetailModels,
            materials: materialUsage.size,
            textures: arenaByName ? includeArenaTextures ? textureList.length : textureReferences.size : textureList.length,
            previewTextures: texturePayloads.length,
            texturedMaterials,
            materialTextureCandidates: includeArenaTextures ? textureList.length : arenaTextureCandidates.size,
            requestedArenaTextures: arenaTextureRequests.length,
            companions: companionSources.length,
            externalTextures: (options.externalTextures || []).length,
            skippedModels: Math.max(0, uniqueModels.length - meshes.length),
            truncated: uniqueModels.length > maxModels || triangles >= maxTriangles,
            arena: arenaLike
          },
          materials: [...materialUsage].map(([name, count]) => ({ name, primitives: count })),
          textures: texturePayloads,
          textureDependencies: textureDependencies.map((dependency) => ({
            ...dependency,
            loaded: texturePayloads.some((texture) => texture.name === dependency.name && texture.source === dependency.source)
          })),
          arenaTextureRequests,
          skipped
        };
      }
      module.exports = {
        buildHairFitBust,
        projectTangentSpaceMeshes: projectTangentSpaceMeshes3,
        buildSceneTransformLookup,
        decodeVertexSkin,
        decodeSparseMorph,
        applyLocalTranslation,
        createIffPreview: createIffPreview3,
        decodeAttribute,
        formatInfo,
        findTransformOffset,
        gameMatrixToPreviewMatrix,
        halfToFloat,
        lookupScenePlacements,
        readIndices,
        sceneNameVariants,
        selectPrimitiveRange,
        textureBinding,
        textureInputColorSpace,
        collectTextureMetadata,
        collectTextureReferences,
        collectArenaMaterialTextures,
        previewTextureScore,
        comparePreviewTextures,
        bodyColorTextureTarget,
        deduplicateModels,
        modelGeometrySignature,
        primitiveMaterial,
        shouldHideArenaModel,
        parseTxtrDocument,
        textureTxtrMetadata,
        txtrRangeMetadata
      };
    }
  });

  // worker/index.js
  init_globals();

  // worker/threads.js
  init_globals();

  // worker/transfer.js
  init_globals();
  function resultBuffers(value) {
    const buffers = /* @__PURE__ */ new Set(), seen = /* @__PURE__ */ new Set();
    const visit = (object) => {
      if (!object || typeof object !== "object" || seen.has(object)) return;
      seen.add(object);
      const buffer = ArrayBuffer.isView(object) ? object.buffer : object;
      if (ArrayBuffer.isView(object) || object instanceof ArrayBuffer) {
        if (buffer instanceof ArrayBuffer && buffer.byteLength) buffers.add(buffer);
        return;
      }
      for (const child of Object.values(object)) visit(child);
    };
    visit(value);
    return [...buffers];
  }

  // worker/threads.js
  var parentPort = {
    on(event, callback) {
      if (event === "message") globalThis.addEventListener("message", ({ data }) => {
        if (!data.kind) callback(data);
      });
    },
    postMessage(value) {
      const buffers = resultBuffers(value.result);
      if (value.result?.stats) value.result.stats.workerTransport = {
        mode: "transfer",
        buffers: buffers.length,
        bytes: buffers.reduce((total, buffer) => total + buffer.byteLength, 0)
      };
      globalThis.postMessage(value, buffers);
    }
  };

  // worker/catalog.js
  init_globals();
  init_native();

  // worker/head-cache.js
  init_globals();
  init_fs();
  var HeadPreviewCache = class {
    constructor() {
      this.entries = /* @__PURE__ */ new Map();
      this.bytes = 0;
    }
    async get(filePath, decode, { clone = true } = {}) {
      const native2 = String(filePath).startsWith("cmt-preview://");
      const resolved = native2 ? filePath : await realpath(filePath);
      let key = resolved;
      if (!native2) {
        const metadata = await stat(resolved);
        const bytes2 = await readFile(resolved);
        const digest = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes2)), (x) => x.toString(16).padStart(2, "0")).join("");
        key = `${resolved.toLowerCase()}|${metadata.size}|${digest}`;
      }
      let entry = this.entries.get(key);
      const cached = !!entry;
      if (!entry) {
        const value = await decode(resolved);
        value.meshes?.forEach((mesh) => {
          mesh.geometryKey ||= `${key}:${mesh.name}`;
        });
        const seen = /* @__PURE__ */ new Set();
        const size = (object) => {
          if (!object || typeof object !== "object" || seen.has(object)) return 0;
          seen.add(object);
          if (ArrayBuffer.isView(object)) return object.byteLength;
          if (object instanceof ArrayBuffer) return object.byteLength;
          return Object.values(object).reduce((total, child) => total + size(child), 0);
        };
        entry = { value, bytes: size(value) };
        if (entry.bytes <= 24 * 1024 * 1024) {
          while (this.entries.size >= 8 || this.bytes + entry.bytes > 24 * 1024 * 1024) {
            const oldest = this.entries.keys().next().value;
            this.bytes -= this.entries.get(oldest).bytes;
            this.entries.delete(oldest);
          }
          this.entries.set(key, entry);
          this.bytes += entry.bytes;
        }
      } else {
        this.entries.delete(key);
        this.entries.set(key, entry);
      }
      return { value: clone ? structuredClone(entry.value) : entry.value, cached };
    }
  };

  // worker/catalog.js
  init_tangent_frame();
  var { createIffPreview } = require_preview();
  var heads = new HeadPreviewCache();
  function hydrate(payload) {
    if (!payload) return null;
    for (const mesh of payload.meshes) {
      for (const [key, value] of Object.entries(mesh)) {
        if (!value || typeof value !== "object" || typeof value.data !== "string" || !["f32", "u32", "u16"].includes(value.kind)) continue;
        const binary = atob(value.data);
        const bytes2 = Uint8Array.from(binary, (char) => char.charCodeAt(0));
        const ArrayType = value.kind === "f32" ? Float32Array : value.kind === "u32" ? Uint32Array : Uint16Array;
        if (bytes2.byteLength % ArrayType.BYTES_PER_ELEMENT) throw new Error("Truncated native geometry array.");
        mesh[key] = new ArrayType(bytes2.buffer);
      }
      for (const key of ["positions", "uvs", "attachmentUvs", "tangentOffsets", "skinWeights", "detailUvs", "overlayUvs"]) {
        if (Array.isArray(mesh[key])) mesh[key] = new Float32Array(mesh[key]);
      }
      for (const key of ["indices", "sourceVertexIds", "tangentAnchorIds", "skinOffsets"]) {
        if (Array.isArray(mesh[key])) mesh[key] = new Uint32Array(mesh[key]);
      }
      if (Array.isArray(mesh.skinBones)) mesh.skinBones = new Uint16Array(mesh.skinBones);
    }
    return payload;
  }
  async function catalog(asset) {
    const started = performance.now();
    const includeBase = asset.includeBase !== false;
    let geometry, fallback;
    if (asset.native && asset.nativeGeometry !== false) {
      try {
        geometry = await native("native-catalog-geometry", { ...asset, includeBase });
      } catch (error) {
        fallback = String(error);
      }
    }
    const hair = await createIffPreview(asset.geometryPath, {
      companionPaths: asset.itemPath ? [{ path: asset.itemPath, kind: "item" }] : [],
      maxTriangles: 35e4,
      maxModels: 80,
      preferHighestDetail: true,
      nativeGeometry: geometry ? hydrate(geometry.asset) : null
    });
    let base, cached, tangentVertices;
    if (geometry) {
      base = hydrate(geometry.base);
      cached = geometry.stats.headCacheHit;
      tangentVertices = geometry.stats.tangentHairProjectedVertices;
    } else {
      const result = await heads.get(asset.sourcePlayerPath, (filePath) => createIffPreview(filePath, {
        hairFitHeadOnly: true,
        skipTextures: true,
        preferHighestDetail: true,
        maxTriangles: 12e4,
        maxModels: 8
      }), { clone: includeBase });
      cached = result.cached;
      base = includeBase ? result.value : null;
      tangentVertices = projectTangentSpaceMeshes(hair, result.value);
      hair.meshes.filter((mesh) => mesh.tangentSpaceAttachment).forEach((mesh) => {
        if (mesh.geometryKey) mesh.geometryKey += ":head:" + (result.value.geometryKey || asset.sourcePlayerPath);
      });
    }
    if (asset.tangentspace && !tangentVertices) throw new Error("No tangent-space vertices could attach to the original donor head.");
    if (fallback) hair.skipped.push("Native catalog geometry fallback: " + fallback);
    const kind = asset.assetType === "facialhair" ? "facialhair-fit-base" : "hair-fit-base";
    if (base) base.meshes.forEach((mesh) => {
      mesh.sourceKind = kind;
    });
    const stats = {
      ...hair.stats,
      ...geometry?.stats || {},
      nativeGeometry: Boolean(geometry),
      headCacheHit: cached,
      tangentHairProjectedVertices: tangentVertices,
      facialHairProjectedVertices: asset.assetType === "facialhair" ? tangentVertices : 0,
      fallbackFrameCache: geometry ? null : frameCacheStats(),
      pickerDecodeMs: Math.round((performance.now() - started) * 10) / 10
    };
    return {
      sourcePng: asset.sourcePng,
      key: asset.key,
      base: base ? { ...base, name: `png${asset.sourcePng} fitting head`, player: { mode: "hair-catalog-native-head", bodyFit: {}, jerseyAvailable: false }, activeItems: [] } : null,
      asset: { ...hair, name: `${asset.key} from png${asset.sourcePng}`, player: { mode: "hair-catalog-asset", bodyFit: {}, jerseyAvailable: false }, activeItems: [asset.key], stats },
      stats
    };
  }

  // worker/staged-projection.mjs
  init_globals();
  var label = (mesh) => [mesh.sourceName, ...(mesh.groups || []).map((group) => group.material)].join(" ");
  var facialHair = (mesh) => Boolean(mesh.tangentOffsets?.length) && /facial.?hair|beard|goatee|scruff/i.test(label(mesh));
  var filename = (mesh) => String(mesh.sourceName || "").replace(/\\/g, "/").split("/").pop().toLowerCase();
  function selectTangentMeshes(meshes, mode) {
    if (mode === "all") return meshes.filter((mesh) => mesh.tangentSpaceAttachment);
    if (mode !== "staged") return meshes.filter(facialHair);
    const ordinary = new Set(meshes.filter((mesh) => !mesh.tangentSpaceAttachment).map(filename));
    return meshes.filter((mesh) => facialHair(mesh) || mesh.tangentSpaceAttachment && /hair/i.test(label(mesh)) && !ordinary.has(filename(mesh).replace(/_tangentspace\.iff$/, ".iff")));
  }

  // worker/index.js
  init_native();
  var { createIffPreview: createIffPreview2, projectTangentSpaceMeshes: projectTangentSpaceMeshes2 } = require_preview();
  async function preview({ filePath, options }) {
    const result = await createIffPreview2(filePath, options);
    if (options.projectTangent) {
      const meshes = selectTangentMeshes(result.meshes, options.projectTangent);
      const count = projectTangentSpaceMeshes2({ meshes }, result);
      result.stats.facialHairProjectedVertices = count;
      const headKey = result.meshes.filter((mesh) => mesh.sourceKind === "main").map((mesh) => mesh.geometryKey || "").join("|");
      meshes.forEach((mesh) => {
        if (mesh.geometryKey) mesh.geometryKey += ":head:" + headKey;
      });
      if (meshes.length && !count) result.skipped.push("Facial-hair geometry could not attach to the staged face tangent frame.");
    }
    return result;
  }
  var queue = [];
  var running = false;
  async function drain() {
    if (running) return;
    running = true;
    try {
      while (queue.length) {
        const { id, type, payload } = queue.shift();
        try {
          if (!["preview", "catalog"].includes(type)) throw new Error("Unknown preview operation.");
          resetNativeTransport();
          const result = await (type === "catalog" ? catalog(payload) : preview(payload));
          result.stats.nativeTransport = nativeTransportStats();
          parentPort.postMessage({ id, result });
        } catch (error) {
          parentPort.postMessage({ id, error: { message: error.message, name: error.name } });
        }
      }
    } finally {
      running = false;
    }
  }
  parentPort.on("message", (message) => {
    if (message.type === "catalog" && message.latestOnly === true) {
      for (let i = queue.length - 1; i >= 0; i--) {
        if (queue[i].type !== "catalog" || queue[i].latestOnly !== true) continue;
        const [superseded] = queue.splice(i, 1);
        parentPort.postMessage({ id: superseded.id, error: {
          message: "Preview request cancelled: newer selection queued.",
          name: "AbortError"
        } });
      }
    }
    queue.push(message);
    void drain();
  });
})();
/*! Bundled license information:

ieee754/index.js:
  (*! ieee754. BSD-3-Clause License. Feross Aboukhadijeh <https://feross.org/opensource> *)

buffer/index.js:
  (*!
   * The buffer module from node.js, for the browser.
   *
   * @author   Feross Aboukhadijeh <https://feross.org>
   * @license  MIT
   *)
*/
