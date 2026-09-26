package com.tabdisplay

import android.view.MotionEvent
import java.nio.ByteBuffer
import kotlin.math.cos
import kotlin.math.sin

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

    /** Null for events that carry nothing for the PC. */
    fun encode(e: MotionEvent, width: Int, height: Int): ByteArray? {
        val action = e.actionMasked
        val changed = e.actionIndex
        val count = e.pointerCount.coerceAtMost(255)
        val buf = ByteBuffer.allocate(1 + count * 18)
        buf.put(count.toByte())
        for (i in 0 until count) {
            val pointerAction = when (action) {
                MotionEvent.ACTION_DOWN -> DOWN
                MotionEvent.ACTION_POINTER_DOWN -> if (i == changed) DOWN else MOVE
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> UP
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

            buf.put(e.getPointerId(i).coerceIn(0, 255).toByte())
            buf.put(if (pen) 1 else 0)
            buf.put(pointerAction.toByte())
            buf.put(buttons.toByte())
            buf.putFloat(e.getX(i) / width)
            buf.putFloat(e.getY(i) / height)
            buf.putFloat(if (pointerAction == HOVER || pointerAction == LEAVE) 0f else e.getPressure(i).coerceIn(0f, 1f))
            buf.put(tiltX.toByte())
            buf.put(tiltY.toByte())
        }
        return buf.array()
    }
}
