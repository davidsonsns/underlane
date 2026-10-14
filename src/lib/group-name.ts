const GROUP_NAME_LIMIT = 64;

export function normalizeGroupName(value: string): string {
  return value.replace(/\p{White_Space}+/gu, " ").trim();
}

export function groupNameError(name: string): string | null {
  if (!name) {
    return "Enter a name.";
  }
  if ([...name].length > GROUP_NAME_LIMIT) {
    return `Use ${GROUP_NAME_LIMIT} characters or fewer.`;
  }
  return null;
}
