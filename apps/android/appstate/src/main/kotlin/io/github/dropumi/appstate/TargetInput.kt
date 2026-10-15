// SPDX-License-Identifier: MPL-2.0
//
// What the user types to reach a device: an address ("Connect by address"),
// an optional fingerprint pin, and a full fingerprint ("Add by
// fingerprint"). These checks only catch typing mistakes early; the core
// checks every value again before it connects or trusts.

package io.github.dropumi.appstate

object TargetInput {
    /** A full fingerprint has 52 base32 characters. */
    const val FINGERPRINT_LENGTH = 52

    /** A pin needs at least this many characters (the core's minimum). */
    const val MIN_PIN_LENGTH = 24

    private const val MAX_PORT = 65_535

    /**
     * The address as the core takes it ("host:port", or "[ipv6]:port"), or
     * null when it is not one: the port must be 1 to 65535, and an IPv6
     * address needs brackets.
     */
    fun address(text: String): String? {
        val value = text.trim()
        val colon = value.lastIndexOf(':')
        if (colon <= 0) return null
        val host = value.substring(0, colon)
        val port = value.substring(colon + 1)
        if (!port.all { it in '0'..'9' } || port.isEmpty() || port.length > 5) return null
        if (port.toInt() !in 1..MAX_PORT) return null
        val ok = if (host.startsWith("[")) isBracketedIpv6(host) else isHostname(host)
        return value.takeIf { ok }
    }

    /** An IPv4 address or a host name, as the core accepts it: ASCII letters, digits, '-' and '.'. */
    private fun isHostname(host: String): Boolean =
        host.isNotEmpty() && host.length <= MAX_HOST_LENGTH && !host.startsWith('-') && !host.startsWith('.') &&
            host.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '-' || it == '.' }

    /** "[fe80::1]" (a zone such as "%wlan0" is allowed). */
    private fun isBracketedIpv6(host: String): Boolean {
        if (!host.endsWith("]") || host.length <= 2) return false
        val inner = host.substring(1, host.length - 1)
        return ':' in inner &&
            inner.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == ':' || it == '.' || it == '%' }
    }

    private const val MAX_HOST_LENGTH = 253

    /** Why [text] is not an address, for the hint under the field; null when it is one or nothing is typed. */
    fun addressProblem(text: String): String? {
        val value = text.trim()
        if (value.isEmpty() || address(value) != null) return null
        val colon = value.lastIndexOf(':')
        if (colon < 0) return "Add the port after a colon, for example 192.168.1.20:47321."
        val port = value.substring(colon + 1)
        val portIsNumber = port.isNotEmpty() && port.all { it in '0'..'9' }
        if (!portIsNumber || port.length > 5 || port.toInt() !in 1..MAX_PORT) {
            return "The port must be a number from 1 to 65535."
        }
        return "Type an address such as 192.168.1.20:47321. Put an IPv6 address in brackets."
    }

    /** The fingerprint without the spaces and dashes people type for reading, in capitals. */
    fun compact(text: String): String = text.filter { !it.isWhitespace() && it != '-' }.uppercase()

    /** The pin to check (null for none). */
    fun pin(text: String): String? = compact(text).ifEmpty { null }

    /** A pin is optional; one that is typed needs 24 to 52 characters. */
    fun pinValid(text: String): Boolean {
        val pin = compact(text)
        return pin.isEmpty() || pin.length in MIN_PIN_LENGTH..FINGERPRINT_LENGTH
    }

    /** The full 52-character fingerprint, or null when [text] is not exactly that long. */
    fun fullFingerprint(text: String): String? = compact(text).takeIf { it.length == FINGERPRINT_LENGTH }
}
