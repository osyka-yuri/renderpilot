const U32_MAX = 0xffff_ffff;
const VERSION_COMPONENT_COUNT = 4;
const CONFIG_PATH = 'bundle.windows.minimumWebview2Version';

export type WebViewRuntimeContract = Readonly<{
  minimumVersion: string;
  major: number;
}>;

export function parseWebViewRuntimeContract(value: unknown): WebViewRuntimeContract {
  if (typeof value !== 'string') {
    throw new TypeError(`${CONFIG_PATH} must be configured as a string.`);
  }

  const parts = value.split('.');
  if (parts.length !== VERSION_COMPONENT_COUNT) {
    throw versionFormatError();
  }

  const components = parts.map((part) => {
    if (!/^\d+$/u.test(part)) {
      throw versionFormatError();
    }

    const component = Number(part);
    if (!Number.isSafeInteger(component) || component > U32_MAX) {
      throw versionFormatError();
    }
    return component;
  });

  const [major] = components;
  if (major === 0) {
    throw new TypeError(`${CONFIG_PATH} major component must be positive.`);
  }

  return Object.freeze({ minimumVersion: value, major });
}

export function edgeBuildTarget(value: unknown): `edge${number}` {
  return `edge${parseWebViewRuntimeContract(value).major}`;
}

function versionFormatError(): TypeError {
  return new TypeError(`${CONFIG_PATH} must contain four numeric u32 components.`);
}
