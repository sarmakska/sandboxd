# Security Policy

I take the security of sandboxd seriously. The whole point of this project is to run untrusted code safely, so a flaw in the isolation boundary is exactly the kind of issue I want to hear about.

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | yes |
| < 0.1 | no |

## Reporting a vulnerability

Please report security issues privately by email to security@sarmalinux.com. Do not open a public issue for a vulnerability.

When you report, please include:

- a description of the issue and the impact you believe it has,
- the steps or a minimal module that reproduces it,
- the sandboxd version and the platform you observed it on.

## My commitment

I will acknowledge your report within 7 days of receiving it. After that I will keep you updated on my assessment and, where a fix is needed, on the timeline for a release. Once a fix ships I am happy to credit you in the changelog unless you would prefer to remain anonymous.

## Scope

In scope: any way for a guest module to escape the configured limits, reach host state that was not explicitly granted, or read or corrupt host memory through the host ABI.

Out of scope: side-channel and timing attacks, denial of service that stays within the configured limits, and vulnerabilities in wasmtime itself (please report those upstream, though I am glad to help coordinate). See the Threat Model page in the wiki for the full boundary.
