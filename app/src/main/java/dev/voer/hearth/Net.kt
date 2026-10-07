package dev.voer.hearth

class ApiException(val code: Int, msg: String) : Exception(msg)
