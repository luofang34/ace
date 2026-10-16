// SPDX-License-Identifier: AGPL-3.0-only
module probe;

import gzip;
import std.exception : enforce;
import std.stdio : File, stdout;

void main(string[] args) {
    enforce(args.length == 3, "probe read|lines|write PATH");
    switch (args[1]) {
        case "read":
            auto bytes = new GzipInputRange(args[2]);
            char[32768] buffer;
            while (!bytes.empty) {
                size_t length;
                while (length < buffer.length && !bytes.empty) {
                    buffer[length++] = bytes.front;
                    bytes.popFront();
                }
                stdout.rawWrite(buffer[0 .. length]);
            }
            break;
        case "lines":
            foreach (line; new GzipByLine(args[2])) {
                stdout.write(line, "\n");
            }
            break;
        case "write":
            import std.stdio : stdin;
            auto writer = new GzipOut(args[2]);
            foreach (chunk; stdin.byChunk(1237)) writer.compress(cast(string) chunk);
            writer.finish();
            writer.finish();
            break;
        default: throw new Exception("unknown probe operation");
    }
}
