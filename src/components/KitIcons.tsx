// Small marks for the starter kit choices. Drawn simply, in each project's colours.

export function ReactMark({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="-12 -12 24 24" aria-hidden>
      <g fill="none" stroke="#61dafb" strokeWidth="1.1">
        <ellipse rx="10" ry="3.9" />
        <ellipse rx="10" ry="3.9" transform="rotate(60)" />
        <ellipse rx="10" ry="3.9" transform="rotate(120)" />
      </g>
      <circle r="1.9" fill="#61dafb" />
    </svg>
  );
}

export function VueMark({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <path d="M1.5 3h4.2L12 13.8 18.3 3h4.2L12 21z" fill="#41b883" />
      <path d="M5.7 3h3.9L12 7.1 14.4 3h3.9L12 13.8z" fill="#35495e" />
    </svg>
  );
}

export function SvelteMark({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <path
        d="M16.8 4.2c-2-1.3-4.7-.8-6.1 1.1L6.9 10.6a4.3 4.3 0 0 0 1.2 6.1c2 1.3 4.7.8 6.1-1.1"
        fill="none"
        stroke="#ff3e00"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
      <path
        d="M7.2 19.8c2 1.3 4.7.8 6.1-1.1l3.8-5.3a4.3 4.3 0 0 0-1.2-6.1c-2-1.3-4.7-.8-6.1 1.1"
        fill="none"
        stroke="#ff3e00"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function LivewireMark({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <path
        d="M4 11a8 8 0 0 1 16 0v4.5c0 1-.8 1.6-1.6 1.2-.6-.3-1.3.1-1.4.8l-.3 2.3c-.1.9-1.3 1-1.6.2l-.6-1.6c-.3-.7-1.2-.7-1.5 0l-.6 1.6c-.3.8-1.5.7-1.6-.2l-.3-2.3c-.1-.7-.8-1.1-1.4-.8-.8.4-1.7-.2-1.7-1.2z"
        fill="#fb70a9"
      />
      <circle cx="9.3" cy="10.2" r="2.1" fill="#fff" />
      <circle cx="9.8" cy="10.4" r="1" fill="#1d1a12" />
      <circle cx="14.7" cy="10.2" r="2.1" fill="#fff" />
      <circle cx="15.2" cy="10.4" r="1" fill="#1d1a12" />
    </svg>
  );
}
