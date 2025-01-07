# Security Policy

Security vulnerabilities should be disclosed to the [project maintainers](./CODEOWNERS), or alternatively by email to defender-support@openzeppelin.com.

## Supported Versions

The following versions are currently supported and receive security updates.
Release candidates will not receive security updates.

Security patches will be released for the latest minor of a given major release. For example, if an issue is found in versions >=1.13.0 and the latest is 1.14.0, the patch will be released only in version 1.14.1.

Only critical severity bug fixes will be backported to past major releases.

| Version   | Supported          |
| --------- | ------------------ |
| >= 1.14.x | :white_check_mark: |
| >= 1.13.x | :white_check_mark: |
| <=1.12.x  | :x:                |

## Reporting a Vulnerability

We're extremely grateful for security researchers and users that report vulnerabilities to us.
All reports are thoroughly investigated by the project's security team.

Vulnerabilities are reported privately via GitHub's [Security Advisories](https://docs.github.com/en/code-security/security-advisories) feature.
Please use the following link to submit your vulnerability: [Report a vulnerability](https://github.com/openzeppelin/defender-sdk/security/advisories/new)

Please see
[Privately reporting a security vulnerability](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability#privately-reporting-a-security-vulnerability)
for more information on how to submit a vulnerability using GitHub's interface.

We highly recommend installing the packages through npm and setting up vulnerability alerts such as [Dependabot].

[Dependabot]: https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/about-supply-chain-security#what-is-dependabot

## Legal

Smart contracts are a nascent technology and carry a high level of technical risk and uncertainty. OpenZeppelin's Defender SDK is made available under the MIT License, which disclaims all warranties in relation to the project and which limits the liability of those that contribute and maintain the project, including OpenZeppelin. Your use of the project is also governed by the terms found at www.openzeppelin.com/tos (the "Terms"). As set out in the Terms, you are solely responsible for any use of OpenZeppelin Defender SDK and you assume all risks associated with any such use. This Security Policy in no way evidences or represents an on-going duty by any contributor, including OpenZeppelin, to correct any flaws or alert you to all or any of the potential risks of utilizing the project.
