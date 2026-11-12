export const IS_MAC = navigator.userAgent.includes("Mac");

export const keys = (mac: string, other: string) => (IS_MAC ? mac : other);
