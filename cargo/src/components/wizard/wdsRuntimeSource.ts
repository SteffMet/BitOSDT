import type { OutputConfig } from './types';

export type WdsRuntimeSource = 'UNC' | 'HTTP';

function hasValue(value?: string): boolean {
  return !!value?.trim();
}

function normalizeUncCredentials(
  output: Pick<OutputConfig, 'fullIsoUncPath' | 'fullIsoUncUsername' | 'fullIsoUncPassword'>,
): Pick<OutputConfig, 'fullIsoUncUsername' | 'fullIsoUncPassword'> {
  if (!hasValue(output.fullIsoUncPath)) {
    return {
      fullIsoUncUsername: '',
      fullIsoUncPassword: '',
    };
  }

  return {
    fullIsoUncUsername: output.fullIsoUncUsername ?? '',
    fullIsoUncPassword: output.fullIsoUncPassword ?? '',
  };
}

export function inferWdsRuntimeSource(output: Pick<OutputConfig, 'wdsRuntimeSource' | 'fullIsoUncPath' | 'fullIsoHttpUrl'>): WdsRuntimeSource {
  const hasUnc = hasValue(output.fullIsoUncPath);
  const hasHttp = hasValue(output.fullIsoHttpUrl);

  if (hasHttp && !hasUnc) {
    return 'HTTP';
  }

  if (output.wdsRuntimeSource === 'HTTP') {
    return 'HTTP';
  }

  return 'UNC';
}

export function normalizeWdsPxeOutput(output: OutputConfig): OutputConfig {
  if (output.outputType !== 'WDSPXE') {
    const uncCredentials = normalizeUncCredentials(output);
    return {
      ...output,
      wdsRuntimeSource: output.wdsRuntimeSource === 'HTTP' ? 'HTTP' : 'UNC',
      ...uncCredentials,
    };
  }

  const hasUnc = hasValue(output.fullIsoUncPath);
  const hasHttp = hasValue(output.fullIsoHttpUrl);
  let wdsRuntimeSource: WdsRuntimeSource;

  if (hasUnc && hasHttp) {
    wdsRuntimeSource = 'UNC';
  } else if (hasHttp) {
    wdsRuntimeSource = 'HTTP';
  } else {
    wdsRuntimeSource = 'UNC';
  }

  return {
    ...output,
    wdsRuntimeSource,
    fullIsoUncPath: wdsRuntimeSource === 'HTTP' ? '' : (output.fullIsoUncPath ?? ''),
    fullIsoUncUsername:
      wdsRuntimeSource === 'HTTP' || !hasUnc ? '' : (output.fullIsoUncUsername ?? ''),
    fullIsoUncPassword:
      wdsRuntimeSource === 'HTTP' || !hasUnc ? '' : (output.fullIsoUncPassword ?? ''),
    fullIsoHttpUrl: wdsRuntimeSource === 'UNC' ? '' : (output.fullIsoHttpUrl ?? ''),
  };
}
