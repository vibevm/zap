/** Latest-request fence for scoped browser reads. @scope spec://org.vibevm.zap/lens/PROP-010#product-copy */
export interface ScopedRequestToken {
  readonly scope: string;
  readonly generation: number;
}

export class ScopedRequestFence {
  #scope = "";
  #generation = 0;

  begin(scope: string): ScopedRequestToken {
    if (scope !== this.#scope) {
      this.#scope = scope;
      this.#generation += 1;
    }
    this.#generation += 1;
    return { scope, generation: this.#generation };
  }

  isCurrent(token: ScopedRequestToken): boolean {
    return token.scope === this.#scope && token.generation === this.#generation;
  }

  cancel(): void {
    this.#generation += 1;
  }
}
