// SPDX-License-Identifier: MPL-2.0
//
// A small strict JSON reader and writer for the app's own history file, so
// the history needs no serialization library and no Android framework class
// (org.json is not available on the host JVM). Input is bounded by the
// caller; nesting is bounded here.

package io.github.dropumi.appstate

internal object Json {
    private const val MAX_DEPTH = 8

    class SyntaxException(message: String) : Exception(message)

    /** Parses one JSON value: Map, List, String, Long (or the digits as a String if too large), Boolean or null. */
    fun parse(text: String): Any? {
        val reader = Reader(text)
        val value = reader.value(0)
        reader.skipSpace()
        if (!reader.atEnd()) throw SyntaxException("trailing data")
        return value
    }

    fun write(value: Any?): String = StringBuilder().also { append(it, value) }.toString()

    private fun append(out: StringBuilder, value: Any?) {
        when (value) {
            null -> out.append("null")
            is Boolean -> out.append(value)
            is Int, is Long -> out.append(value)
            is ULong -> out.append(value.toString())
            is String -> appendString(out, value)
            is Map<*, *> -> {
                out.append('{')
                value.entries.forEachIndexed { i, (k, v) ->
                    if (i > 0) out.append(',')
                    appendString(out, k as String)
                    out.append(':')
                    append(out, v)
                }
                out.append('}')
            }
            is List<*> -> {
                out.append('[')
                value.forEachIndexed { i, v ->
                    if (i > 0) out.append(',')
                    append(out, v)
                }
                out.append(']')
            }
            else -> throw IllegalArgumentException("unsupported JSON value")
        }
    }

    private fun appendString(out: StringBuilder, s: String) {
        out.append('"')
        for (c in s) {
            when {
                c == '"' -> out.append("\\\"")
                c == '\\' -> out.append("\\\\")
                c < ' ' || c == ' ' || c == ' ' -> out.append("\\u%04x".format(c.code))
                else -> out.append(c)
            }
        }
        out.append('"')
    }

    private class Reader(private val s: String) {
        private var i = 0

        fun atEnd() = i >= s.length

        fun skipSpace() {
            while (i < s.length && s[i] in " \t\r\n") i++
        }

        fun value(depth: Int): Any? {
            if (depth > MAX_DEPTH) throw SyntaxException("too deep")
            skipSpace()
            if (atEnd()) throw SyntaxException("unexpected end")
            return when (s[i]) {
                '{' -> objectValue(depth)
                '[' -> arrayValue(depth)
                '"' -> string()
                't' -> literal("true", true)
                'f' -> literal("false", false)
                'n' -> literal("null", null)
                else -> number()
            }
        }

        private fun literal(word: String, result: Any?): Any? {
            if (!s.startsWith(word, i)) throw SyntaxException("bad literal")
            i += word.length
            return result
        }

        private fun number(): Any {
            val start = i
            if (i < s.length && s[i] == '-') i++
            val digitsStart = i
            while (i < s.length && s[i].isAsciiDigit()) i++
            if (i == digitsStart) throw SyntaxException("bad number")
            val text = s.substring(start, i)
            return text.toLongOrNull() ?: text
        }

        private fun Char.isAsciiDigit() = this in '0'..'9'

        private fun string(): String {
            i++ // opening quote
            val out = StringBuilder()
            while (true) {
                if (atEnd()) throw SyntaxException("unterminated string")
                val c = s[i++]
                when {
                    c == '"' -> return out.toString()
                    c == '\\' -> {
                        if (atEnd()) throw SyntaxException("bad escape")
                        when (val e = s[i++]) {
                            '"', '\\', '/' -> out.append(e)
                            'b' -> out.append('\b')
                            'f' -> out.append('\u000c')
                            'n' -> out.append('\n')
                            'r' -> out.append('\r')
                            't' -> out.append('\t')
                            'u' -> {
                                if (i + 4 > s.length) throw SyntaxException("bad escape")
                                val code = s.substring(i, i + 4).toIntOrNull(16) ?: throw SyntaxException("bad escape")
                                out.append(code.toChar())
                                i += 4
                            }
                            else -> throw SyntaxException("bad escape")
                        }
                    }
                    c < ' ' -> throw SyntaxException("control character in string")
                    else -> out.append(c)
                }
            }
        }

        private fun arrayValue(depth: Int): List<Any?> {
            i++
            val out = mutableListOf<Any?>()
            skipSpace()
            if (!atEnd() && s[i] == ']') {
                i++
                return out
            }
            while (true) {
                out.add(value(depth + 1))
                skipSpace()
                if (atEnd()) throw SyntaxException("unterminated array")
                when (s[i++]) {
                    ',' -> continue
                    ']' -> return out
                    else -> throw SyntaxException("bad array")
                }
            }
        }

        private fun objectValue(depth: Int): Map<String, Any?> {
            i++
            val out = linkedMapOf<String, Any?>()
            skipSpace()
            if (!atEnd() && s[i] == '}') {
                i++
                return out
            }
            while (true) {
                skipSpace()
                if (atEnd() || s[i] != '"') throw SyntaxException("bad key")
                val key = string()
                skipSpace()
                if (atEnd() || s[i++] != ':') throw SyntaxException("missing colon")
                out[key] = value(depth + 1)
                skipSpace()
                if (atEnd()) throw SyntaxException("unterminated object")
                when (s[i++]) {
                    ',' -> continue
                    '}' -> return out
                    else -> throw SyntaxException("bad object")
                }
            }
        }
    }
}
