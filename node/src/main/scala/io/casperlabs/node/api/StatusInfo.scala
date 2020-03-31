package io.casperlabs.node.api

import cats.effect.Sync
import cats.implicits._
import com.google.protobuf.ByteString
import doobie.util.transactor.Transactor
import io.casperlabs.crypto.codec.Base16
import io.casperlabs.casper.consensus.Block
import io.casperlabs.comm.discovery.NodeDiscovery
import io.casperlabs.node.configuration.Configuration
import org.http4s.HttpRoutes
import io.casperlabs.storage.dag.DagStorage
import io.casperlabs.models.Message
import io.casperlabs.casper.ValidatorIdentity

object StatusInfo {

  case class Status(
      version: String,
      ok: Boolean,
      checklist: CheckList
  )

  trait Check {
    val ok: Boolean
    val message: Option[String]
  }
  object Check {
    case class Checked(val ok: Boolean, val message: Option[String] = None) extends Check
    case class LastBlock(
        ok: Boolean,
        message: Option[String] = None,
        blockHash: Option[String],
        timestamp: Option[Long],
        jRank: Option[Long]
    ) extends Check

    def database[F[_]: Sync](readXa: Transactor[F]) = {
      import doobie._
      import doobie.implicits._
      sql"""select 1""".query[Int].unique.transact(readXa).map { _ =>
        Checked(ok = true)
      }
    }

    def peers[F[_]: Sync: NodeDiscovery](conf: Configuration) =
      NodeDiscovery[F].recentlyAlivePeersAscendingDistance.map { nodes =>
        Checked(
          ok = conf.casper.standalone || nodes.nonEmpty,
          message = s"${nodes.length} recently alive peers.".some
        )
      }

    def bootstrap[F[_]: Sync: NodeDiscovery](conf: Configuration, genesis: Block) = {
      val bootstrapNodes = conf.server.bootstrap.map(_.withChainId(genesis.blockHash))
      NodeDiscovery[F].recentlyAlivePeersAscendingDistance.map(_.toSet).map { nodes =>
        val connected = bootstrapNodes.filter(nodes)
        Checked(
          ok = bootstrapNodes.isEmpty || connected.nonEmpty,
          message = s"Connected to ${connected.size} of the bootstrap nodes.".some
        )
      }
    }

    def lastReceivedBlock[F[_]: Sync: DagStorage](
        conf: Configuration,
        maybeValidatorId: Option[ByteString]
    ): F[LastBlock] =
      for {
        dag      <- DagStorage[F].getRepresentation
        tips     <- dag.latestGlobal
        messages <- tips.latestMessages
        received = messages.values.flatten.filter { m =>
          maybeValidatorId.fold(true)(_ != m.validatorId)
        }
        latest = if (received.nonEmpty) received.maxBy(_.timestamp).some else none
      } yield LastBlock(
        ok = conf.casper.standalone || latest.nonEmpty,
        message = latest.fold("Haven't received any blocks yet.".some)(_ => none),
        blockHash = latest.map(m => Base16.encode(m.messageHash.toByteArray)),
        timestamp = latest.map(_.timestamp),
        jRank = latest.map(_.jRank)
      )

    def lastCreatedBlock[F[_]: Sync: DagStorage](
        maybeValidatorId: Option[ByteString]
    ): F[LastBlock] =
      for {
        created <- maybeValidatorId.fold(Set.empty[Message].pure[F]) { id =>
                    for {
                      dag      <- DagStorage[F].getRepresentation
                      tips     <- dag.latestGlobal
                      messages <- tips.latestMessage(id)
                    } yield messages
                  }
        latest = if (created.nonEmpty) created.maxBy(_.timestamp).some else none
      } yield LastBlock(
        ok = maybeValidatorId.isEmpty || created.nonEmpty,
        message = latest.fold("Haven't created any blocks yet.".some)(_ => none),
        blockHash = latest.map(m => Base16.encode(m.messageHash.toByteArray)),
        timestamp = latest.map(_.timestamp),
        jRank = latest.map(_.jRank)
      )

  }

  case class CheckList(
      database: Check.Checked,
      peers: Check.Checked,
      bootstrap: Check.Checked,
      lastReceivedBlock: Check.LastBlock,
      lastCreatedBlock: Check.LastBlock
  ) {
    // I thought about putting everything in a `List[_ <: Check]` and having a custom Json Encoder to
    // show them as an object, or producing a Json object first and parsing the flags, or using Shapeless.
    // This pedestrian version is the most straight forward, but in the future I'd go with Shapeless.
    def ok = List(database, peers, bootstrap, lastReceivedBlock, lastCreatedBlock).forall(_.ok)
  }

  def status[F[_]: Sync: NodeDiscovery: DagStorage](
      conf: Configuration,
      genesis: Block,
      maybeValidatorId: Option[ByteString],
      readXa: Transactor[F]
  ): F[Status] =
    for {
      version           <- Sync[F].delay(VersionInfo.get)
      database          <- Check.database[F](readXa)
      peers             <- Check.peers[F](conf)
      bootstrap         <- Check.bootstrap[F](conf, genesis)
      lastReceivedBlock <- Check.lastReceivedBlock[F](conf, maybeValidatorId)
      lastCreatedBlock  <- Check.lastCreatedBlock[F](maybeValidatorId)
      checklist = CheckList(
        database = database,
        peers = peers,
        bootstrap = bootstrap,
        lastReceivedBlock = lastReceivedBlock,
        lastCreatedBlock = lastCreatedBlock
      )
    } yield Status(version, checklist.ok, checklist)

  def service[F[_]: Sync: NodeDiscovery: DagStorage](
      conf: Configuration,
      genesis: Block,
      maybeValidatorId: Option[ValidatorIdentity],
      readXa: Transactor[F]
  ): HttpRoutes[F] = {
    import io.circe.generic.auto._
    import io.circe.syntax._
    import org.http4s.circe.CirceEntityEncoder._

    val dsl = org.http4s.dsl.Http4sDsl[F]
    import dsl._

    val maybeValidatorKey = maybeValidatorId.map(id => ByteString.copyFrom(id.publicKey))

    HttpRoutes.of[F] {
      // Could return a different HTTP status code, but it really depends on what we want from this.
      // An 50x would mean the service is kaput, which may be too harsh.
      case GET -> Root =>
        Ok(
          status(conf, genesis, maybeValidatorKey, readXa)
            .map(_.asJson)
        )
    }
  }
}
