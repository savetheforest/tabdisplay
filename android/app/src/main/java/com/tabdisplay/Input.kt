package com.tabdisplay

import android.view.MotionEvent
import java.nio.ByteBuffer
import kotlin.math.cos
import kotlin.math.sin

data class InputContact(
    val id: Int,
    val kind: Int,
    val action: Int,
    val buttons: Int,
    val x: Float,
    val y: Float,
    val pressure: Float,
    val tiltX: Int,
    val tiltY: Int,
)

/** Pure INPUT codec; MotionEvent remains an adapter around this protocol representation. */
object InputFrame {
    private const val CONTACT_SIZE = 18

    fun encode(contacts: List<InputContact>): ByteArray {
        require(contacts.size <= 255) { "too many contacts" }
        val buffer = ByteBuffer.allocate(1 + contacts.size * CONTACT_SIZE)
        buffer.put(contacts.size.toByte())
        val ids = BooleanArray(256)
        contacts.forEach { c ->
            require(!ids[c.id.coerceIn(0, 255)]) { "duplicate contact id" }
            ids[c.id.coerceIn(0, 255)] = true
            require(contacts.size <= 10) { "too many synthetic contacts" }
            require(c.kind in 0..1 && c.action in 0..5 && c.buttons and -4 == 0)
            require(c.x.isFinite() && c.y.isFinite() && c.pressure.isFinite())
            require(c.x in 0f..1f && c.y in 0f..1f && c.pressure in 0f..1f)
            require(c.tiltX in -90..90 && c.tiltY in -90..90)
            buffer.put(c.id.coerceIn(0, 255).toByte())
            buffer.put(c.kind.toByte())
            buffer.put(c.action.toByte())
            buffer.put(c.buttons.toByte())
            buffer.putFloat(c.x).putFloat(c.y).putFloat(c.pressure)
            buffer.put(c.tiltX.toByte()).put(c.tiltY.toByte())
        }
        return buffer.array()
    }

    fun parse(payload: ByteArray): List<InputContact>? {
        if (payload.isEmpty()) return null
        val count = payload[0].toInt() and 0xff
        if (payload.size != 1 + count * CONTACT_SIZE) return null
        val buffer = ByteBuffer.wrap(payload)
        buffer.position(1)
        val contacts = ArrayList<InputContact>(count)
        val ids = BooleanArray(256)
        repeat(count) {
            val contact = InputContact(
                id = buffer.get().toInt() and 0xff,
                kind = buffer.get().toInt() and 0xff,
                action = buffer.get().toInt() and 0xff,
                buttons = buffer.get().toInt() and 0xff,
                x = buffer.float,
                y = buffer.float,
                pressure = buffer.float,
                tiltX = buffer.get().toInt(),
                tiltY = buffer.get().toInt(),
            )
            if (ids[contact.id] || contact.kind !in 0..1 || contact.action !in 0..5 || contact.buttons and -4 != 0 ||
                    !contact.x.isFinite() || !contact.y.isFinite() || !contact.pressure.isFinite() ||
                    contact.x !in 0f..1f || contact.y !in 0f..1f || contact.pressure !in 0f..1f ||
                    contact.tiltX !in -90..90 || contact.tiltY !in -90..90
            ) return null
            ids[contact.id] = true
            contacts += contact
        }
        return contacts
    }
}

/**
 * Encodes a MotionEvent as an INPUT frame (PROTOCOL.md): `[count]` then per contact
 * `[id][kind 0 touch/1 pen][action][buttons 1 barrel/2 eraser][x f32][y f32][pressure f32][tiltX i8][tiltY i8]`,
 * with x/y normalized to the view. Windows reproduces the frame as native touch/pen, so gestures and
 * pen pressure come for free on the PC side.
 */
object Input {
    private const val DOWN = 0
    private const val MOVE = 1
    private const val UP = 2
    private const val HOVER = 3
    private const val LEAVE = 4
    private const val CANCEL = 5

    /** A mouse wheel / trackpad scroll as a SCROLL payload `[x f32][y f32][dx f32][dy f32]`; null for other events. */
    fun scroll(e: MotionEvent, width: Int, height: Int): ByteArray? {
        if (e.actionMasked != MotionEvent.ACTION_SCROLL) return null
        return ByteBuffer.allocate(16)
            .putFloat(e.x / width).putFloat(e.y / height)
            .putFloat(e.getAxisValue(MotionEvent.AXIS_HSCROLL)).putFloat(e.getAxisValue(MotionEvent.AXIS_VSCROLL))
            .array()
    }

    /** Null for events that carry nothing for the PC. */
    fun encode(e: MotionEvent, width: Int, height: Int): ByteArray? {
        val action = e.actionMasked
        val changed = e.actionIndex
        if (e.pointerCount > 10) return null // matches the Windows synthetic touch device capacity
        val count = e.pointerCount
        val contacts = ArrayList<InputContact>(count)
        for (i in 0 until count) {
            val pointerAction = when (action) {
                MotionEvent.ACTION_DOWN -> DOWN
                MotionEvent.ACTION_POINTER_DOWN -> if (i == changed) DOWN else MOVE
                MotionEvent.ACTION_UP -> UP
                MotionEvent.ACTION_CANCEL -> CANCEL
                MotionEvent.ACTION_POINTER_UP -> if (i == changed) UP else MOVE
                MotionEvent.ACTION_MOVE -> MOVE
                MotionEvent.ACTION_HOVER_ENTER, MotionEvent.ACTION_HOVER_MOVE -> HOVER
                MotionEvent.ACTION_HOVER_EXIT -> LEAVE
                else -> return null
            }
            val tool = e.getToolType(i)
            val pen = tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER
            if (!pen && (pointerAction == HOVER || pointerAction == LEAVE)) return null // mouse hover etc.
            var buttons = 0
            if (e.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY != 0) buttons = buttons or 1
            if (tool == MotionEvent.TOOL_TYPE_ERASER) buttons = buttons or 2
            // Tilt: angle from vertical plus direction; split into X/Y degrees like Windows expects.
            val tilt = e.getAxisValue(MotionEvent.AXIS_TILT, i)
            val orientation = e.getAxisValue(MotionEvent.AXIS_ORIENTATION, i)
            val tiltX = Math.toDegrees((tilt * sin(orientation)).toDouble()).toInt().coerceIn(-90, 90)
            val tiltY = Math.toDegrees((-tilt * cos(orientation)).toDouble()).toInt().coerceIn(-90, 90)

            contacts += InputContact(
                id = e.getPointerId(i).coerceIn(0, 255),
                kind = if (pen) 1 else 0,
                action = pointerAction,
                buttons = buttons,
                x = (e.getX(i) / width).coerceIn(0f, 1f),
                y = (e.getY(i) / height).coerceIn(0f, 1f),
                pressure = if (pointerAction == HOVER || pointerAction == LEAVE) 0f else e.getPressure(i).coerceIn(0f, 1f),
                tiltX = tiltX,
                tiltY = tiltY,
            )
        }
        return InputFrame.encode(contacts)
    }
}
