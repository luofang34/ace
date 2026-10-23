// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Aircraft Concept Explorer contributors
// Streaming Eilmer file adapters over zlib's RFC 1952 encoder and decoder.
module gzip;

import etc.c.zlib;
import std.array : appender;
import std.exception : enforce;
import std.stdio : File;

/// Byte range over all members of a gzip file, with CRC and length validation.
class GzipInputRange {
    private File input;
    private string path;
    private z_stream decoder;
    private ubyte[32768] encoded;
    private ubyte[32768] decoded;
    private size_t cursor, available;
    private bool initialized, memberEnded, finished;

    this(string filename) {
        path = filename;
        input = File(filename, "rb");
        enforce(inflateInit2(&decoder, 31) == Z_OK, "gzip decoder initialization: " ~ path);
        initialized = true;
        scope(failure) close();
        refill();
    }

    ~this() { close(); }

    // Eilmer explicitly closes files through this public accessor.
    @property ref File f() { return input; }

    private void close() {
        if (initialized) {
            inflateEnd(&decoder);
            initialized = false;
        }
        if (input.isOpen) input.close();
    }

    @property bool empty() const { return finished; }

    @property char front() const {
        enforce(!empty, "read beyond gzip end: " ~ path);
        return cast(char) decoded[cursor];
    }

    void popFront() {
        enforce(!empty, "advance beyond gzip end: " ~ path);
        cursor++;
        if (cursor == available) refill();
    }

    private void refill() {
        cursor = available = 0;
        while (available == 0) {
            if (decoder.avail_in == 0) {
                auto bytes = input.rawRead(encoded[]);
                if (bytes.length == 0) {
                    enforce(memberEnded, "truncated gzip member: " ~ path);
                    finished = true;
                    close();
                    return;
                }
                decoder.next_in = bytes.ptr;
                decoder.avail_in = cast(uint) bytes.length;
            }
            if (memberEnded) {
                // RFC 1952 permits concatenated members, including empty ones.
                enforce(inflateReset(&decoder) == Z_OK, "gzip member reset: " ~ path);
                memberEnded = false;
            }
            decoder.next_out = decoded.ptr;
            decoder.avail_out = cast(uint) decoded.length;
            auto before = decoder.avail_in;
            auto status = inflate(&decoder, Z_NO_FLUSH);
            enforce(status == Z_OK || status == Z_STREAM_END,
                    "invalid gzip header, compressed data or checksum: " ~ path);
            available = decoded.length - decoder.avail_out;
            memberEnded = status == Z_STREAM_END;
            enforce(available != 0 || decoder.avail_in < before || memberEnded,
                    "gzip decoder made no progress: " ~ path);
        }
    }
}

/// Lines without LF terminators; blank lines and an unterminated final line survive.
class GzipByLine {
    private GzipInputRange bytes;
    private string line;
    private bool atEnd;

    @property GzipInputRange range() { return bytes; }

    this(string filename) {
        bytes = new GzipInputRange(filename);
        readLine();
    }

    @property bool empty() const { return atEnd; }

    string front() const {
        enforce(!atEnd, "read beyond gzip line range");
        return line;
    }

    void popFront() {
        enforce(!atEnd, "advance beyond gzip line range");
        readLine();
    }

    private void readLine() {
        atEnd = bytes.empty;
        auto result = appender!string();
        while (!bytes.empty) {
            char value = bytes.front;
            bytes.popFront();
            if (value == '\n') break;
            result.put(value);
        }
        line = result.data;
    }
}

/// Incremental gzip writer. finish() writes the trailer and closes the file.
class GzipOut {
    private File output;
    private z_stream encoder;
    private ubyte[32768] encoded;
    private bool initialized;
    private bool finished;

    this(string filename) {
        output = File(filename, "wb");
        enforce(deflateInit2(&encoder, Z_DEFAULT_COMPRESSION, Z_DEFLATED,
                            31, 8, Z_DEFAULT_STRATEGY) == Z_OK,
                "gzip encoder initialization: " ~ filename);
        initialized = true;
    }

    ~this() {
        if (initialized) deflateEnd(&encoder);
        if (output.isOpen) output.close();
    }

    void compress(string value) {
        enforce(!finished, "cannot write to a finished gzip stream");
        while (value.length != 0) {
            uint count = cast(uint) (value.length < encoded.length ? value.length : encoded.length);
            encoder.next_in = cast(ubyte*) value.ptr;
            encoder.avail_in = count;
            // Consume every byte before returning: callers can reuse their input buffer.
            while (encoder.avail_in != 0) emit(Z_NO_FLUSH);
            value = value[count .. $];
        }
    }

    void finish() {
        if (finished) return;
        while (emit(Z_FINISH) != Z_STREAM_END) {}
        enforce(deflateEnd(&encoder) == Z_OK, "gzip encoder cleanup failed");
        initialized = false;
        output.flush();
        output.close();
        finished = true;
    }

    private int emit(int mode) {
        encoder.next_out = encoded.ptr;
        encoder.avail_out = cast(uint) encoded.length;
        int status = deflate(&encoder, mode);
        enforce(status == Z_OK || status == Z_STREAM_END, "gzip compression failed");
        output.rawWrite(encoded[0 .. encoded.length - encoder.avail_out]);
        return status;
    }
}
