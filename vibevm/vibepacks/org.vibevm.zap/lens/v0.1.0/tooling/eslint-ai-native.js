/**
 * Vendored JavaScript build of the complete Scaffold-F rule from
 * org.vibevm.ai-native/typescript-ai-native-lang v1.0.0,
 * tools/eslint-plugin-ai-native/src/{req-message,diagnostic-cites-req}.ts.
 * Type annotations alone were removed so an extracted npm package has no
 * host-relative dependency and Node does not type-strip code in node_modules.
 */
const PREFIX = "violates REQ ";
const FIX_MARKER = "; fix surface: ";
const KNOWN_SCHEMES = ["spec://", "discipline://", "misra://"];

export function reqMessage(uri, why, fixSurface) {
  return `violates REQ ${uri}: ${why}; fix surface: ${fixSurface}`;
}

export function matchesReqGrammar(message) {
  if (!message.startsWith(PREFIX)) return false;
  const rest = message.slice(PREFIX.length);
  const knownScheme = KNOWN_SCHEMES.some((scheme) => rest.startsWith(scheme));
  return knownScheme && rest.includes(": ") && rest.includes(FIX_MARKER);
}

function isErrorCalleeName(name) {
  return /(?:Error|Exception)$/.test(name);
}

function calleeName(callee) {
  if (callee.type === "Identifier") return callee.name;
  if (callee.type === "MemberExpression") {
    return callee.property.type === "Identifier" ? callee.property.name : null;
  }
  return null;
}

function errorMessageLiteral(node) {
  if (node.type !== "NewExpression") return null;
  const name = calleeName(node.callee);
  if (name === null || !isErrorCalleeName(name)) return null;
  const first = node.arguments[0];
  return first?.type === "Literal" && typeof first.value === "string" ? first : null;
}

function thrownObjectMessageLiteral(node) {
  const argument = node.argument;
  if (argument === null || argument.type !== "ObjectExpression") return null;
  for (const property of argument.properties) {
    if (
      property.type === "Property" &&
      !property.computed &&
      property.key.type === "Identifier" &&
      property.key.name === "message" &&
      property.value.type === "Literal" &&
      typeof property.value.value === "string"
    ) {
      return property.value;
    }
  }
  return null;
}

function snippet(value) {
  const oneLine = value.replace(/\s+/g, " ").trim();
  return oneLine.length > 60 ? `${oneLine.slice(0, 57)}...` : oneLine;
}

const REQ_URI =
  "discipline://typescript-ai-native-lang/cards/scaffold-f-structured-diagnostics#ops";

export const diagnosticCitesReq = {
  meta: {
    type: "problem",
    docs: {
      description:
        "A project-raised diagnostic must cite the violated requirement and a fix surface.",
      url: "discipline://typescript-ai-native-lang/cards/scaffold-f-structured-diagnostics",
    },
    schema: [],
    messages: {},
  },
  defaultOptions: [],
  create(context) {
    function check(what, literal) {
      if (literal === null || matchesReqGrammar(literal.value)) return;
      context.report({
        node: literal,
        message: reqMessage(
          REQ_URI,
          `${what} is free text: ${snippet(literal.value)}`,
          "render it with reqMessage(<REQ URI>, why, fix) so it cites the violated REQ and a one-line fix surface",
        ),
      });
    }
    return {
      NewExpression(node) {
        check("custom diagnostic", errorMessageLiteral(node));
      },
      ThrowStatement(node) {
        check("thrown-object diagnostic", thrownObjectMessageLiteral(node));
      },
    };
  },
};

export default {
  meta: { name: "@org.vibevm/eslint-plugin-ai-native", version: "0.1.0-vendored-js" },
  rules: { "diagnostic-cites-req": diagnosticCitesReq },
};
