/** Fail-closed default managed environment resolver. @scope spec://org.vibevm.zap/lens/PROP-010#provider-identity */
export function createDefaultManagedEnvironment() {
  return {
    resolve(reference: string | null) {
      return Promise.resolve(
        reference === null
          ? { ok: true as const, value: {} }
          : {
              ok: false as const,
              message: "managed environment references require a trusted runtime provider",
            },
      );
    },
  };
}
