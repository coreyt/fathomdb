import { spawn } from "node:child_process";
import type { Readable, Writable } from "node:stream";
import type { QueryEmbedder } from "./index.js";

export interface SubprocessEmbedderOptions {
  command: string[];
  dimensions: number;
  identityOverride?: string;
}

interface SpawnedProc {
  stdin: Writable;
  stdout: Readable;
  exitCode: number | null;
  killed: boolean;
  on(event: "exit", cb: () => void): void;
  on(event: "error", cb: () => void): void;
}

export class SubprocessEmbedder implements QueryEmbedder {
  private readonly _command: string[];
  private readonly _dimensions: number;
  private readonly _identity: string;
  private _proc: SpawnedProc | null = null;

  constructor(options: SubprocessEmbedderOptions) {
    this._command = options.command;
    this._dimensions = options.dimensions;
    this._identity = options.identityOverride ?? options.command.join(" ");
  }

  identity(): string {
    return this._identity;
  }

  maxTokens(): number {
    return 512;
  }

  private _ensureProc(): SpawnedProc {
    if (this._proc === null || this._proc.exitCode !== null || this._proc.killed) {
      const [cmd, ...args] = this._command;
      const proc = spawn(cmd, args, {
        stdio: ["pipe", "pipe", "inherit"],
      });
      proc.on("exit", () => {
        if (this._proc === (proc as unknown as SpawnedProc)) {
          this._proc = null;
        }
      });
      proc.on("error", () => {
        if (this._proc === (proc as unknown as SpawnedProc)) {
          this._proc = null;
        }
      });
      this._proc = proc as unknown as SpawnedProc;
    }
    return this._proc;
  }

  private _readExact(stdout: Readable, n: number): Promise<Buffer> {
    return new Promise((resolve, reject) => {
      const chunks: Buffer[] = [];
      let received = 0;

      const onData = (chunk: Buffer | string) => {
        const chunkBuf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk as string, "binary");
        chunks.push(chunkBuf);
        received += chunkBuf.length;
        if (received >= n) {
          stdout.off("data", onData);
          stdout.off("error", onError);
          stdout.off("end", onEnd);
          const combined = Buffer.concat(chunks);
          resolve(combined.subarray(0, n));
        }
      };

      const onError = (err: Error) => {
        stdout.off("data", onData);
        stdout.off("end", onEnd);
        reject(err);
      };

      const onEnd = () => {
        stdout.off("data", onData);
        stdout.off("error", onError);
        reject(
          new Error(
            `SubprocessEmbedder: subprocess closed stdout after ${received} bytes (expected ${n})`,
          ),
        );
      };

      stdout.on("data", onData);
      stdout.once("error", onError);
      stdout.once("end", onEnd);
    });
  }

  async embed(texts: string[]): Promise<number[][]> {
    const results: number[][] = [];

    for (const text of texts) {
      // Re-acquire process each iteration (supports restart on exit)
      const proc = this._ensureProc();
      const byteCount = this._dimensions * 4;

      // Start listening for data BEFORE writing, to avoid missing buffered output
      const readPromise = this._readExact(proc.stdout, byteCount);

      const input = Buffer.from(text + "\n", "utf-8");
      await new Promise<void>((resolve, reject) => {
        proc.stdin.write(input, (err) => {
          if (err) reject(err);
          else resolve();
        });
      });

      const data = await readPromise;

      const vec: number[] = [];
      for (let i = 0; i < this._dimensions; i++) {
        vec.push(data.readFloatLE(i * 4));
      }
      results.push(vec);
    }

    return results;
  }
}
